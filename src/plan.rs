//! The `plan` crate provides functionality for creating spending plans.

use signature::PublicKey;
use chrono::prelude::*;
use std::mem;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum Condition {
    Timestamp(DateTime<Utc>),
    Signature(PublicKey),
}

pub enum PlanEvent {
    Timestamp(DateTime<Utc>),
    Signature(PublicKey),
}

impl Condition {
    pub fn is_satisfied(&self, event: &PlanEvent) -> bool {
        match (self, event) {
            (&Condition::Signature(ref pubkey), &PlanEvent::Signature(ref from)) => pubkey == from,
            (&Condition::Timestamp(ref dt), &PlanEvent::Timestamp(ref last_time)) => {
                dt <= last_time
            }
            _ => false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum Action {
    Pay(Payment),
}

impl Action {
    pub fn spendable(&self) -> i64 {
        match *self {
            Action::Pay(ref payment) => payment.asset,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Payment {
    pub asset: i64,
    pub to: PublicKey,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum Plan {
    Action(Action),
    After(Condition, Action),
    Race((Condition, Action), (Condition, Action)),
}

impl Plan {
    pub fn new_payment(asset: i64, to: PublicKey) -> Self {
        Plan::Action(Action::Pay(Payment { asset, to }))
    }

    pub fn new_authorized_payment(from: PublicKey, asset: i64, to: PublicKey) -> Self {
        Plan::After(
            Condition::Signature(from),
            Action::Pay(Payment { asset, to }),
        )
    }

    pub fn new_future_payment(dt: DateTime<Utc>, asset: i64, to: PublicKey) -> Self {
        Plan::After(Condition::Timestamp(dt), Action::Pay(Payment { asset, to }))
    }

    pub fn new_cancelable_future_payment(
        dt: DateTime<Utc>,
        from: PublicKey,
        asset: i64,
        to: PublicKey,
    ) -> Self {
        Plan::Race(
            (Condition::Timestamp(dt), Action::Pay(Payment { asset, to })),
            (
                Condition::Signature(from),
                Action::Pay(Payment { asset, to: from }),
            ),
        )
    }

    pub fn verify(&self, spendable_assets: i64) -> bool {
        match *self {
            Plan::Action(ref action) => action.spendable() == spendable_assets,
            Plan::After(_, ref action) => action.spendable() == spendable_assets,
            Plan::Race(ref a, ref b) => {
                a.1.spendable() == spendable_assets && b.1.spendable() == spendable_assets
            }
        }
    }

    pub fn process_event(&mut self, event: PlanEvent) -> bool {
        let mut new_action = None;
        match *self {
            Plan::Action(_) => return true,
            Plan::After(ref cond, ref action) => {
                if cond.is_satisfied(&event) {
                    new_action = Some(action.clone());
                }
            }
            Plan::Race(ref a, ref b) => {
                if a.0.is_satisfied(&event) {
                    new_action = Some(a.1.clone());
                } else if b.0.is_satisfied(&event) {
                    new_action = Some(b.1.clone());
                }
            }
        }

        if let Some(action) = new_action {
            mem::replace(self, Plan::Action(action));
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_satisfied() {
        let sig = PublicKey::default();
        assert!(Condition::Signature(sig).is_satisfied(&PlanEvent::Signature(sig)));
    }

    #[test]
    fn test_timestamp_satisfied() {
        let dt1 = Utc.ymd(2014, 11, 14).and_hms(8, 9, 10);
        let dt2 = Utc.ymd(2014, 11, 14).and_hms(10, 9, 8);
        assert!(Condition::Timestamp(dt1).is_satisfied(&PlanEvent::Timestamp(dt1)));
        assert!(Condition::Timestamp(dt1).is_satisfied(&PlanEvent::Timestamp(dt2)));
        assert!(!Condition::Timestamp(dt2).is_satisfied(&PlanEvent::Timestamp(dt1)));
    }

    #[test]
    fn test_verify_plan() {
        let dt = Utc.ymd(2014, 11, 14).and_hms(8, 9, 10);
        let from = PublicKey::default();
        let to = PublicKey::default();
        assert!(Plan::new_payment(42, to).verify(42));
        assert!(Plan::new_authorized_payment(from, 42, to).verify(42));
        assert!(Plan::new_future_payment(dt, 42, to).verify(42));
        assert!(Plan::new_cancelable_future_payment(dt, from, 42, to).verify(42));
    }

    #[test]
    fn test_authorized_payment() {
        let from = PublicKey::default();
        let to = PublicKey::default();

        let mut plan = Plan::new_authorized_payment(from, 42, to);
        assert!(plan.process_event(PlanEvent::Signature(from)));
        assert_eq!(plan, Plan::new_payment(42, to));
    }

    #[test]
    fn test_future_payment() {
        let dt = Utc.ymd(2014, 11, 14).and_hms(8, 9, 10);
        let to = PublicKey::default();

        let mut plan = Plan::new_future_payment(dt, 42, to);
        assert!(plan.process_event(PlanEvent::Timestamp(dt)));
        assert_eq!(plan, Plan::new_payment(42, to));
    }

    #[test]
    fn test_cancelable_future_payment() {
        let dt = Utc.ymd(2014, 11, 14).and_hms(8, 9, 10);
        let from = PublicKey::default();
        let to = PublicKey::default();

        let mut plan = Plan::new_cancelable_future_payment(dt, from, 42, to);
        assert!(plan.process_event(PlanEvent::Timestamp(dt)));
        assert_eq!(plan, Plan::new_payment(42, to));

        let mut plan = Plan::new_cancelable_future_payment(dt, from, 42, to);
        assert!(plan.process_event(PlanEvent::Signature(from)));
        assert_eq!(plan, Plan::new_payment(42, from));
    }
}