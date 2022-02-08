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
pub use pallet::*;
use pallet_common::*;
use scale_info::TypeInfo;
use sp_std::prelude::*;
use xcm::v0::{Junction, OriginKind, SendXcm, Xcm};

#[cfg(feature = "std")]
use frame_support::serde::{Deserialize, Serialize};
use sp_std::convert::{TryFrom, TryInto};

use cumulus_primitives_core::{relay_chain, ParaId};
use xcm::VersionedXcm;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub type MomentOf<T> = <T as pallet_timestamp::Config>::Moment;

type Timestamp<T> = pallet_timestamp::Pallet<T>;

pub(crate) type OrderOf<T> = Order<
    <T as Config>::OrderPayload,
    BalanceOf<T>,
    MomentOf<T>,
    <T as frame_system::Config>::AccountId,
    ParaId,
>;

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

	}

	// Struct for holding device information.
	#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct DeviceProfile<T: Config> {
        penalty: BalanceOf<T>,
        work_duration: MomentOf<T>,
        para_id: ParaId,
        device_state: DeviceState,
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
	pub type Orders<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, OrderOf<T>, OptionQuery>;


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
}
