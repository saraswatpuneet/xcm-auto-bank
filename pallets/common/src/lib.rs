#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};
use scale_info::TypeInfo;

use frame_support::{
    sp_runtime::traits::Hash,
    sp_runtime::RuntimeDebug,
    traits::{BalanceStatus::Free, Currency, Get, ReservableCurrency},
};

#[cfg_attr(feature = "std", derive(Debug))]
#[derive(Encode, Decode, PartialEq, TypeInfo)]
pub enum DeviceState {
    /// Device is off
    Off,
    /// Device is ready to accept orders
    Ready,
    /// Device has order
    Busy,
    /// Device has accepted order
    Accepted,
    /// Device is abandoned
    Timewait,
}
impl Default for DeviceState {
    fn default() -> Self {
        DeviceState::Off
    }
}

#[derive(Encode, Decode, Default, Clone, RuntimeDebug, PartialEq, TypeInfo)]
pub struct OrderBase<Payload: Encode + Decode, Balance, Moment, AccountId> {
    pub until: Moment,
    pub data: Payload,
    pub fee: Balance,
    pub device: AccountId,
}

impl<Payload: Encode + Decode, Balance, Moment, AccountId>
    OrderBase<Payload, Balance, Moment, AccountId>
{
    pub fn convert<ParaId: From<u32>>(
        self,
        client: AccountId,
    ) -> Order<Payload, Balance, Moment, AccountId, ParaId> {
        Order {
            until: self.until,
            data: self.data,
            fee: self.fee,
            client,
            paraid: 0.into(),
        }
    }
}

#[derive(Encode, Decode, Default, Clone, RuntimeDebug, PartialEq, TypeInfo)]
pub struct Order<Payload: Encode + Decode, Balance, Moment, AccountId, ParaId> {
    pub until: Moment,
    pub data: Payload,
    pub fee: Balance,
    pub client: AccountId,
    pub paraid: ParaId,
}

impl<Payload: Encode + Decode, Balance, Moment, AccountId, ParaId>
    Order<Payload, Balance, Moment, AccountId, ParaId>
{
    pub fn convert(self, device: AccountId) -> OrderBase<Payload, Balance, Moment, AccountId> {
        OrderBase {
            until: self.until,
            data: self.data,
            fee: self.fee,
            device,
        }
    }
}

#[derive(codec::Encode, codec::Decode)]
pub enum XCMPMessage<XAccountId, XBalance, Payout: Encode + Decode, Moment> {
    NewOrder(XAccountId, OrderBase<Payout, XBalance, Moment, XAccountId>),
    OrderAccept(XAccountId, XAccountId),
    OrderReject(XAccountId, XAccountId, bool),
    OrderDone(XAccountId, XAccountId, bool),
}
