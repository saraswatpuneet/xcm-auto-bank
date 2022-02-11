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
use cumulus_primitives_core::ParaId;

use frame_support::traits::OnKilledAccount;
pub use pallet::*;
use pallet_common::*;
use scale_info::TypeInfo;
use sp_std::prelude::*;

#[cfg(feature = "std")]
use frame_support::serde::{Deserialize, Serialize};
use sp_std::convert::{TryFrom, TryInto};

use cumulus_primitives_core::{
	relay_chain, relay_chain::BlockNumber as RelayBlockNumber, ServiceQuality,
	XcmpMessageFormat, XcmpMessageHandler,
};

use xcm::VersionedXcm;

type XCMPMessageOf<T> = XCMPMessage<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T>,
	<T as Config>::OrderPayload,
	<T as pallet_timestamp::Config>::Moment,
>;

pub(crate) type OrderBaseOf<T> = OrderBase<
	<T as Config>::OrderPayload,
	BalanceOf<T>,
	MomentOf<T>,
	<T as frame_system::Config>::AccountId,
>;

pub(crate) type OrderOf<T> = Order<
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

		type XcmpMessageSender: SendXcm;
	}

	// Struct for holding device information.
	#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct DeviceProfile<T: Config> {
		pub penalty: BalanceOf<T>,
		pub work_duration: MomentOf<T>,
		pub para_id: ParaId,
		pub device_state: DeviceState,
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
		NewOrder(T::AccountId, T::AccountId),
		Accept(T::AccountId, T::AccountId),
		Reject(T::AccountId, T::AccountId),
		Done(T::AccountId, T::AccountId),
		BadVersion(<T as frame_system::Config>::Hash),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		NoneValue,
		OrderExists,
		IllegalState,
		Overdue,
		DeviceLowBail,
		DeviceExists,
		BadOrderDetails,
		NoDevice,
		NoOrder,
		Prohibited,
		CannotReachDestination,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn test(origin: OriginFor<T>) -> DispatchResult {
			let dest = (Parent, Parachain(200));
			let call: Vec<u8> = vec![0x00, 0x20].encode();
			let message = Xcm(vec![Instruction::Transact {
				origin_type: OriginKind::Native,
				require_weight_at_most: 0,
				call: call.into(),
			}]);
			T::XcmpMessageSender::send_xcm(dest, message)
				.map_err(|_| Error::<T>::CannotReachDestination.into())
				.map(|_| ())
		}
		#[pallet::weight(10_000)]
		pub fn order(origin: OriginFor<T>, order: OrderBaseOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let now = Timestamp::<T>::get();

			if now >= order.until {
				return Err(Error::<T>::Overdue.into());
			}
			if Orders::<T>::contains_key(&order.device) {
				return Err(Error::<T>::IllegalState.into());
			};
			let mut dev = Device::<T>::get(&order.device).ok_or(Error::<T>::NoDevice)?;

			if dev.device_state != DeviceState::Ready {
				return Err(Error::<T>::IllegalState.into());
			}
			if order.until < (now + dev.work_duration) {
				return Err(Error::<T>::BadOrderDetails.into());
			};
			if !T::Currency::can_reserve(&who, order.fee) {
				return Err(Error::<T>::DeviceLowBail.into());
			}

			T::Currency::reserve(&order.device, dev.penalty)?;
			T::Currency::reserve(&who, order.fee)?;
			let device = order.device.clone();
			// store order
			let order: OrderBaseOf<T> = {
				let order: OrderOf<T> = order.convert(who.clone());
				Orders::<T>::insert(&device, &order);
				order.convert(device.clone())
			};
			let msg: XCMPMessageOf<T> = XCMPMessageOf::<T>::NewOrder(who.clone(), order);
			let dest = (Parent, Parachain(dev.para_id.into()));
			let call = msg.encode();
			let message = Xcm(vec![Instruction::Transact {
				origin_type: OriginKind::Native,
				require_weight_at_most: 0,
				call: call.into(),
			}]);
			log::info!("send XCM order message");
			T::XcmpMessageSender::send_xcm(dest, message)
				.map_err(|_| Error::<T>::CannotReachDestination)
				.map(|_| ());
			log::info!("XCM order message has sent");
			dev.device_state = DeviceState::Busy;
			Device::<T>::insert(&device, &dev);

			Self::deposit_event(Event::NewOrder(who, device.clone()));

			Ok(())
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
			Self::order_reject(who, &order, now, device, &mut dev)
		}

		#[pallet::weight(10_000)]
		pub fn register(
			origin: OriginFor<T>,
			paraid: ParaId,
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
					work_duration: wcd,
					penalty,
					device_state: if onoff { DeviceState::Ready } else { DeviceState::Off },
					para_id: paraid,
				},
			);

			Self::deposit_event(Event::NewDevice(id));
			Ok(())
		}
	}
}
impl<T: Config> Pallet<T> {
	fn on_accept(who: T::AccountId, device: T::AccountId) -> DispatchResult {
		Self::deposit_event(Event::Accept(who, device));
		Ok(())
	}

	fn on_reject(who: T::AccountId, device: T::AccountId, onoff: bool) -> DispatchResult {
		let order = Orders::<T>::get(&device).ok_or(Error::<T>::NoOrder)?;

		let now = Timestamp::<T>::get();
		let mut dev = Device::<T>::get(&device).ok_or(Error::<T>::NoDevice)?;

		dev.device_state = if !onoff { DeviceState::Off } else { DeviceState::Ready };

		Self::order_reject(who, &order, now, device, &mut dev)
	}

	fn on_done(who: T::AccountId, device: T::AccountId, onoff: bool) -> DispatchResult {
		let order = Orders::<T>::get(&device).ok_or(Error::<T>::NoOrder)?;
		let now = Timestamp::<T>::get();
		let mut dev = Device::<T>::get(&device).ok_or(Error::<T>::NoDevice)?;

		T::Currency::repatriate_reserved(&who, &device, order.fee, Free)?;

		if now < order.until {
			T::Currency::unreserve(&device, dev.penalty);
		} else {
			T::Currency::repatriate_reserved(&device, &who, dev.penalty, Free)?;
		}
		Orders::<T>::remove(&device);

		dev.device_state = if !onoff { DeviceState::Off } else { DeviceState::Ready };

		Device::<T>::insert(&device, &dev);
		Self::deposit_event(Event::Done(who, device));
		Ok(())
	}
	fn order_reject(
		who: T::AccountId,
		order: &OrderOf<T>,
		now: T::Moment,
		device: T::AccountId,
		dev: &mut DeviceProfile<T>,
	) -> DispatchResult {
		T::Currency::unreserve(&who, order.fee);

		if now < order.until {
			T::Currency::unreserve(&device, dev.penalty);
		} else {
			T::Currency::repatriate_reserved(&device, &order.client, dev.penalty, Free)?;
		}

		Orders::<T>::remove(&device);
		Device::<T>::insert(&device, &*dev);

		Self::deposit_event(Event::Reject(who, device));
		Ok(())
	}
}
impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	/// The account with the given id was reaped.
	fn on_killed_account(who: &T::AccountId) {
		//Timewait
		if let Some(mut dev) = Device::<T>::get(who) {
			if dev.device_state == DeviceState::Off {
				Device::<T>::remove(who);
			} else {
				dev.device_state = DeviceState::Timewait;
				Device::<T>::insert(who, dev);
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
			match XCMPMessageOf::<T>::decode(&mut data_ref) {
				Err(e) => {
					log::error!("{:?}", e);
					return 0;
				},
				Ok(XCMPMessageOf::<T>::OrderAccept(client, devid)) => {
					Self::on_accept(client, devid);
					log::info!("OrderAccept");
				},
				Ok(XCMPMessageOf::<T>::OrderReject(client, devid, onoff)) => {
					Self::on_reject(client, devid, onoff);
					log::info!("OrderReject");
				},
				Ok(XCMPMessageOf::<T>::OrderDone(cliend, devid, onoff)) => {
					Self::on_done(cliend, devid, onoff);
					log::info!("OrderDone");
				},
				Ok(_) => {
					log::warn!("unknown XCM message received");
				},
			};
		}
		max_weight
	}
}
