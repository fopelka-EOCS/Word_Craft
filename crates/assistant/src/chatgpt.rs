//! Sanitized host interface. Credentials never enter provider settings or UI state.
use crate::Http;
use serde::Serialize;

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connected: bool,
    pub busy: bool,
    pub account: Option<String>,
    pub usage_acknowledged: bool,
    /// Only a completed inference establishes account eligibility.
    pub verified_request: bool,
    pub error: Option<String>,
    /// Changes on every account operation; not an account identifier.
    pub generation: u64,
}

pub trait Connection {
    fn status(&self) -> Status;
    fn sign_in(&mut self);
    fn cancel(&mut self);
    fn disconnect(&mut self);
    fn acknowledge_usage(&mut self);
    /// Drop pending work; late results cannot be delivered to a new document/account.
    fn cancel_requests(&mut self);
    fn http(&self) -> &Http;
}
