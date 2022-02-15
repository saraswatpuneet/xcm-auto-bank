#![cfg_attr(not(feature = "std"), no_std)]
#![allow(unused_imports)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/v3/runtime/frame>
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use frame_support::{
	dispatch::DispatchResult,
	sp_runtime::traits::Hash,
	sp_runtime::RuntimeDebug,
	traits::{BalanceStatus::Free, Currency, Get, ReservableCurrency},
};

use xcm::latest::{prelude::*, Junction, MultiLocation, OriginKind, SendXcm, Xcm};

use frame_support::traits::OnKilledAccount;
pub use pallet::*;
pub use pallet_common::*;
use scale_info::TypeInfo;
use sp_std::prelude::*;

#[cfg(feature = "std")]
use frame_support::serde::{Deserialize, Serialize};
use sp_std::convert::{TryFrom, TryInto};

use cumulus_primitives_core::{
	relay_chain, relay_chain::BlockNumber as RelayBlockNumber, ParaId, ServiceQuality,
	XcmpMessageFormat, XcmpMessageHandler,
};

use xcm::VersionedXcm;

pub trait OnReceived<T: Config> {
	fn on_received(
		device: &<T as frame_system::Config>::AccountId,
		order: &OrderOf<T>,
	) -> Option<DeviceState>;
}

type XCMPMessageOf<T> = XCMPMessage<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T>,
	<T as Config>::OrderPayload,
	<T as pallet_timestamp::Config>::Moment,
>;

pub type OrderBaseOf<T> = OrderBase<
	<T as Config>::OrderPayload,
	BalanceOf<T>,
	MomentOf<T>,
	<T as frame_system::Config>::AccountId,
>;

pub type OrderOf<T> = Order<
	<T as Config>::OrderPayload,
	BalanceOf<T>,
	MomentOf<T>,
	<T as frame_system::Config>::AccountId,
	ParaId,
>;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub type MomentOf<T> = <T as pallet_timestamp::Config>::Moment;

type Timestamp<T> = pallet_timestamp::Pallet<T>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_timestamp::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: ReservableCurrency<Self::AccountId>;

		type OrderPayload: Encode + Decode + Clone + Default + Parameter + TypeInfo;
		type SelfParaId: Get<ParaId>;

		type XcmpMessageSender: SendXcm;

		type OnReceived: OnReceived<Self>;
	}

	// Struct for holding device information.
	#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct DeviceProfile<T: Config> {
		pub penalty: BalanceOf<T>,
		pub wcd: MomentOf<T>,
		pub state: DeviceState,
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// Device profiles
	#[pallet::storage]
	#[pallet::getter(fn devices)]
	pub type Device<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, DeviceProfile<T>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn orders)]
	pub type Orders<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, OrderOf<T>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		NewDevice(T::AccountId),
		NewOrder(T::AccountId),
		Accept(T::AccountId),
		Reject(T::AccountId),
		Done(T::AccountId),
		BadVersion(<T as frame_system::Config>::Hash),
		MessageReceived(Vec<u8>),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		NoneValue,
		Prohibited,
		DeviceExists,
		DeviceLowBail,
		NoOrder,
		BadOrderDetails,
		NoDevice,
		IllegalState,
		Overdue,
		CannotReachDestination,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn order(origin: OriginFor<T>, order: OrderBaseOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let OrderBaseOf::<T> { data, until, fee, device } = order;
			let order =
				OrderOf::<T> { fee, data, until, paraid: T::SelfParaId::get(), client: who.into() };

			Self::order_received(order, device)
		}

		#[pallet::weight(10_000)]
		pub fn cancel(origin: OriginFor<T>, device: T::AccountId) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let order = Orders::<T>::get(&device).ok_or(Error::<T>::NoOrder)?;

			let now = Timestamp::<T>::get();

			if now < order.until || order.client != who {
				return Err(Error::<T>::Prohibited.into());
			}

			let mut dev = Device::<T>::get(&device).ok_or(Error::<T>::NoDevice)?;
			// Note. we don't change device state
			Self::order_reject(Some(&order), now, device, &mut dev, false)
		}
		#[pallet::weight(10_000)]
		pub fn accept(origin: OriginFor<T>, reject: bool, onoff: bool) -> DispatchResult {
			let id = ensure_signed(origin)?;

			let order = Orders::<T>::get(&id);

			let mut dev = Device::<T>::get(&id).ok_or(Error::<T>::NoDevice)?;

			let now = Timestamp::<T>::get();
			if reject {
				if !matches!(dev.state, DeviceState::Busy | DeviceState::Accepted) {
					return Err(Error::<T>::IllegalState.into());
				}
				dev.state = if onoff { DeviceState::Ready } else { DeviceState::Off };
				return Self::order_reject(order.as_ref(), now, id, &mut dev, onoff);
			}
			if dev.state != DeviceState::Busy {
				return Err(Error::<T>::IllegalState.into());
			}
			let order = order.ok_or(Error::<T>::NoOrder)?;

			if now >= order.until {
				return Err(Error::<T>::Overdue.into());
			}

			Self::order_accept(&order, now, id, &mut dev);
			Ok(())
		}
		#[pallet::weight(10_000)]
		pub fn done(origin: OriginFor<T>, onoff: bool) -> DispatchResult {
			let id = ensure_signed(origin)?;

			let mut dev = Device::<T>::get(&id).ok_or(Error::<T>::NoDevice)?;

			if dev.state != DeviceState::Accepted {
				return Err(Error::<T>::IllegalState.into());
			}

			let order = Orders::<T>::take(&id).ok_or(Error::<T>::NoOrder)?;
			let now = Timestamp::<T>::get();

			Self::order_done(&order, now, id, &mut dev, onoff)
		}

		#[pallet::weight(10_000)]
		pub fn register(
			origin: OriginFor<T>,
			penalty: BalanceOf<T>,
			wcd: MomentOf<T>,
			onoff: bool,
		) -> DispatchResult {
			let id = ensure_signed(origin)?;

			if Orders::<T>::contains_key(&id) {
				return Err(Error::<T>::DeviceExists.into());
			}
			// Despite the order doesn't exist, device can be in Busy,Busy2 state.
			//
			Device::<T>::insert(
				&id,
				DeviceProfile {
					wcd,
					penalty,
					state: if onoff { DeviceState::Ready } else { DeviceState::Off },
				},
			);
			Self::deposit_event(Event::NewDevice(id));
			Ok(())
		}

		#[pallet::weight(10_000)]
		pub fn set_state(origin: OriginFor<T>, onoff: bool) -> DispatchResult {
			let id = ensure_signed(origin)?;

			Device::<T>::try_mutate(&id, |d| {
				if let Some(ref mut dev) = d {
					dev.state = match (onoff, &dev.state) {
						(false, DeviceState::Ready | DeviceState::Off) => DeviceState::Off,
						//(false, DeviceState::Busy2 | DeviceState::Standby) => DeviceState::Standby,
						(true, DeviceState::Off) => DeviceState::Ready,
						_ => return Err(Error::<T>::IllegalState.into()),
					};
					Ok(())
				} else {
					Err(Error::<T>::NoDevice.into())
				}
			})
		}
	}
}
impl<T: Config> Pallet<T> {
	pub fn order_received(order: OrderOf<T>, device: T::AccountId) -> DispatchResult {
		let now = Timestamp::<T>::get();

		if now >= order.until {
			return Err(Error::<T>::Overdue.into());
		}

		if Orders::<T>::contains_key(&device) {
			return Err(Error::<T>::IllegalState.into());
		}

		let mut dev = Device::<T>::get(&device).ok_or(Error::<T>::NoDevice)?;
		if dev.state != DeviceState::Ready {
			return Err(Error::<T>::IllegalState.into());
		}

		if order.until < (now + dev.wcd) {
			return Err(Error::<T>::BadOrderDetails.into());
		}

		dev.state = T::OnReceived::on_received(&device, &order).ok_or(Error::<T>::IllegalState)?;

		debug_assert!(matches!(dev.state, DeviceState::Busy | DeviceState::Accepted));

		if order.paraid == T::SelfParaId::get() {
			if !T::Currency::can_reserve(&order.client, order.fee) {
				return Err(Error::<T>::DeviceLowBail.into());
			}
			T::Currency::reserve(&device, dev.penalty)?;
			T::Currency::reserve(&order.client, order.fee)?;
		}

		Orders::<T>::insert(&device, &order);
		Self::deposit_event(Event::NewOrder(device.clone()));

		if dev.state == DeviceState::Accepted {
			Self::order_accept(&order, now, device, &mut dev);
		} else {
			Device::<T>::insert(&device, &dev);
		}
		Ok(())
	}
	fn order_done(
		order: &OrderOf<T>,
		now: T::Moment,
		device: T::AccountId,
		dev: &mut DeviceProfile<T>,
		onoff: bool,
	) -> DispatchResult {
		dev.state = if onoff { DeviceState::Ready } else { DeviceState::Off };

		let para_id = T::SelfParaId::get();
		Device::<T>::insert(&device, &*dev);

		if order.paraid == para_id {
			T::Currency::repatriate_reserved(&order.client, &device, order.fee, Free)?;

			if now < order.until {
				T::Currency::unreserve(&device, dev.penalty);
			} else {
				T::Currency::repatriate_reserved(&device, &order.client, dev.penalty, Free)?;
			}
		} else {
			log::info!("send OrderDone message");
			let msg: XCMPMessageOf<T> =
				XCMPMessageOf::<T>::OrderDone(order.client.clone(), device.clone(), onoff);
			let msg: XCMPMessageOf<T> =
				XCMPMessageOf::<T>::OrderDone(order.client.clone(), device.clone(), onoff);
			let dest = (Parent, Parachain(order.paraid.into()));
			let call = msg.encode();
			let message = Xcm(vec![Instruction::Transact {
				origin_type: OriginKind::Native,
				require_weight_at_most: 0,
				call: call.into(),
			}]);
			T::XcmpMessageSender::send_xcm(dest, message)
				.map_err(|_| Error::<T>::CannotReachDestination)
				.map(|_| ());
			log::info!("OrderDone's sent");
		}

		Self::deposit_event(Event::Done(device));
		Ok(())
	}

	fn order_accept(
		order: &OrderOf<T>,
		_now: T::Moment,
		device: T::AccountId,
		dev: &mut DeviceProfile<T>,
	) {
		dev.state = DeviceState::Accepted;
		Device::<T>::insert(&device, &*dev);
		let para_id = T::SelfParaId::get();

		if order.paraid != para_id {
			let msg: XCMPMessageOf<T> =
				XCMPMessageOf::<T>::OrderAccept(order.client.clone(), device.clone());
			let dest = (Parent, Parachain(order.paraid.into()));
			let call = msg.encode();
			let message = Xcm(vec![Instruction::Transact {
				origin_type: OriginKind::Native,
				require_weight_at_most: 0,
				call: call.into(),
			}]);
			T::XcmpMessageSender::send_xcm(dest, message)
				.map_err(|_| Error::<T>::CannotReachDestination)
				.map(|_| ());
		}

		Self::deposit_event(Event::Accept(device));
	}

	fn order_reject(
		order: Option<&OrderOf<T>>,
		now: T::Moment,
		device: T::AccountId,
		dev: &mut DeviceProfile<T>,
		onoff: bool,
	) -> DispatchResult {
		if let Some(order) = order {
			let para_id = T::SelfParaId::get();

			if order.paraid == para_id {
				T::Currency::unreserve(&order.client, order.fee);
				if now < order.until {
					T::Currency::unreserve(&device, dev.penalty);
				} else {
					T::Currency::repatriate_reserved(&device, &order.client, dev.penalty, Free)?;
				}
			} else {
				log::info!("send OrderReject message");
				let msg: XCMPMessageOf<T> =
					XCMPMessageOf::<T>::OrderReject(order.client.clone(), device.clone(), onoff);
				let dest = (Parent, Parachain(order.paraid.into()));
				let call = msg.encode();
				let message = Xcm(vec![Instruction::Transact {
					origin_type: OriginKind::Native,
					require_weight_at_most: 0,
					call: call.into(),
				}]);
				T::XcmpMessageSender::send_xcm(dest, message)
					.map_err(|_| Error::<T>::CannotReachDestination)
					.map(|_| ());
				log::info!("OrderReject's sent");
			}
		}
		Orders::<T>::remove(&device);

		Device::<T>::insert(&device, &*dev);
		Self::deposit_event(Event::Reject(device));

		Ok(())
	}
}

impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(who: &T::AccountId) {
		if let Some(dev) = Device::<T>::get(who) {
			if dev.state == DeviceState::Off {
				Device::<T>::remove(who);
			}
		}
	}
}

impl<T: Config> XcmpMessageHandler for Pallet<T> {
	fn handle_xcmp_messages<'a, I: Iterator<Item = (ParaId, RelayBlockNumber, &'a [u8])>>(
		iter: I,
		max_weight: Weight,
	) -> Weight {
		for (sender, sent_at, data) in iter {
			let mut data_ref = data;
			log::warn!("Processing Blob XCM: {:?}", data_ref);
			match XCMPMessageOf::<T>::decode(&mut data_ref) {
				Err(e) => {
					log::error!("{:?}", e);
					return 0;
				},
				Ok(XCMPMessageOf::<T>::NewOrder(client, order)) => {
					let OrderBaseOf::<T> { data, until, fee, device } = order;
					let order = OrderOf::<T> { fee, data, until, paraid: sender, client };
					log::info!("new order received for {:?}", &device);
					match Self::order_received(order, device) {
						Err(e) => {
							log::error!("order_received return {:?}", e);
						},
						Ok(_) => {
							log::info!("order_received succeed");
						},
					}
				},
				Ok(_) => {
					log::warn!("unknown XCMP message received");
					return 0;
				},
			};
		}
		max_weight
	}
}
