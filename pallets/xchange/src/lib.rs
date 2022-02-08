#![cfg_attr(not(feature = "std"), no_std)]
#![allow(unused_imports)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/v3/runtime/frame>
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::DispatchResult,
	sp_runtime::traits::Hash,
	sp_runtime::RuntimeDebug,
	traits::{BalanceStatus::Free, Currency, Get, ReservableCurrency},
};
use xcm::latest::{prelude::*, Junction, MultiLocation, OriginKind, SendXcm, Xcm};

use frame_support::traits::OnKilledAccount;
pub use pallet::*;
use pallet_common::*;
use scale_info::TypeInfo;
use sp_std::prelude::*;

#[cfg(feature = "std")]
use frame_support::serde::{Deserialize, Serialize};
use sp_std::convert::{TryFrom, TryInto};

use cumulus_primitives_core::{relay_chain, ParaId, ServiceQuality, XcmpMessageHandler};

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

		type OrderPayload: Encode + Decode + Clone + Default + Parameter + MaxEncodedLen + TypeInfo;

		type XcmpMessageSender: SendXcm;
	}

	// Struct for holding device information.
	#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct DeviceProfile<T: Config> {
		pub penalty: BalanceOf<T>,
		pub work_duration: MomentOf<T>,
		pub para_id: ParaId,
		pub device_state: DeviceState,
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
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
            Ok(())
        }
        #[pallet::weight(10_000)]
        pub fn order(origin: OriginFor<T>, order: OrderBaseOf<T>) -> DispatchResult {
            Ok(())
        }

        #[pallet::weight(10_000)]
        pub fn cancel(origin: OriginFor<T>, device: T::AccountId) -> DispatchResult {
            Ok(())
        }
        
        #[pallet::weight(10_000)]
        pub fn register(
            origin: OriginFor<T>,
            paraid: ParaId,
            penalty: BalanceOf<T>,
            wcd: MomentOf<T>,
            onoff: bool,
        ) -> DispatchResult {
            Ok(())
        }
        
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
