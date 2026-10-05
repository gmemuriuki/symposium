//! Daily admission budget shared by named aggregate families.

use crate::telemetry::schema::UtcDay;

use super::open_day::{DayBeforeCurrent, OpenDay, OpenDayUpdate};

/// Maximum public rows one aggregate family may admit in a UTC day.
pub(in crate::telemetry) const MAX_PUBLIC_ROWS_PER_DAY: u64 = 128;

/// Whether a new public aggregate keeps its identity or joins overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PublicRowAdmission {
    Public,
    Overflow,
}

/// Independent daily allowance for one family of public aggregate rows.
///
/// Callers check for an existing aggregate before consuming this budget.
/// Identifier reset deliberately does not mutate it. Day rollover and clear
/// are the only operations that restore the full allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DailyPublicRowBudget {
    day: OpenDay,
    admitted: u64,
}

impl DailyPublicRowBudget {
    #[must_use]
    pub(super) const fn new(day: UtcDay) -> Self {
        Self {
            day: OpenDay::new(day),
            admitted: 0,
        }
    }

    /// Select a day, restoring the allowance after forward UTC-day rollover.
    ///
    /// # Errors
    ///
    /// Returns [`DayBeforeCurrent`] if the observed day precedes the
    /// current open UTC day.
    pub(super) fn select_day(&mut self, day: UtcDay) -> Result<OpenDayUpdate, DayBeforeCurrent> {
        let update = self.day.select(day)?;
        if update == OpenDayUpdate::Advanced {
            self.admitted = 0;
        }

        Ok(update)
    }

    /// Reconcile the current allowance with public rows surviving on disk.
    ///
    /// Day advancement happens first so a closed day's admissions never
    /// consume the new day's allowance. The larger count is retained because
    /// private state covers rows lost from the snapshot, while the snapshot
    /// covers private state lost after its rows were published.
    ///
    /// # Errors
    ///
    /// Returns [`DayBeforeCurrent`] if `day` precedes the current open day.
    pub(super) fn reconcile(
        &mut self,
        day: UtcDay,
        surviving_public_rows: u64,
    ) -> Result<OpenDayUpdate, DayBeforeCurrent> {
        let update = self.select_day(day)?;
        self.admitted = self
            .admitted
            .max(surviving_public_rows.min(MAX_PUBLIC_ROWS_PER_DAY));
        Ok(update)
    }

    /// Consume one allowance slot for a new public aggregate.
    #[must_use]
    pub(super) const fn admit_new(&mut self) -> PublicRowAdmission {
        if self.admitted >= MAX_PUBLIC_ROWS_PER_DAY {
            return PublicRowAdmission::Overflow;
        }

        self.admitted = self
            .admitted
            .checked_add(1)
            .expect("BUG: the public-row allowance is bounded before incrementing");
        PublicRowAdmission::Public
    }

    /// Return whether a new public aggregate must join overflow.
    #[must_use]
    pub(super) const fn is_exhausted(self) -> bool {
        self.admitted >= MAX_PUBLIC_ROWS_PER_DAY
    }

    /// Restore the current day's allowance after telemetry clear.
    pub(super) const fn clear(&mut self) {
        self.admitted = 0;
    }

    #[cfg(test)]
    #[must_use]
    pub(super) const fn admitted(self) -> u64 {
        self.admitted
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn day(day: u32) -> UtcDay {
        UtcDay::from_date(NaiveDate::from_ymd_opt(2026, 8, day).unwrap())
    }

    #[test]
    fn the_128th_public_row_is_admitted_and_the_129th_overflows() {
        let mut budget = DailyPublicRowBudget::new(day(3));

        for admitted in 1..=MAX_PUBLIC_ROWS_PER_DAY {
            assert_eq!(budget.admit_new(), PublicRowAdmission::Public);
            assert_eq!(budget.admitted(), admitted);
        }
        assert_eq!(budget.admit_new(), PublicRowAdmission::Overflow);

        assert_eq!(budget.admitted(), MAX_PUBLIC_ROWS_PER_DAY);
    }

    #[test]
    fn a_forward_day_restores_the_allowance_but_the_current_day_does_not() {
        let mut budget = DailyPublicRowBudget::new(day(3));
        assert_eq!(budget.admit_new(), PublicRowAdmission::Public);

        assert_eq!(budget.select_day(day(3)), Ok(OpenDayUpdate::Current));
        assert_eq!(budget.admitted(), 1);
        assert_eq!(budget.select_day(day(4)), Ok(OpenDayUpdate::Advanced));

        assert_eq!(budget.admitted(), 0);
    }

    #[test]
    fn an_older_day_is_rejected_without_changing_the_budget() {
        let mut budget = DailyPublicRowBudget::new(day(4));
        assert_eq!(budget.admit_new(), PublicRowAdmission::Public);
        let before = budget;

        let result = budget.select_day(day(3));

        assert_eq!(
            result,
            Err(DayBeforeCurrent {
                current: day(4),
                observed: day(3),
            })
        );
        assert_eq!(budget, before);
    }

    #[test]
    fn clear_restores_the_current_days_allowance() {
        let mut budget = DailyPublicRowBudget::new(day(3));
        assert_eq!(budget.admit_new(), PublicRowAdmission::Public);

        budget.clear();

        assert_eq!(budget.admitted(), 0);
        assert_eq!(budget.select_day(day(3)), Ok(OpenDayUpdate::Current));
    }

    #[test]
    fn reconciliation_keeps_the_larger_private_or_surviving_count() {
        let mut private_ahead = DailyPublicRowBudget::new(day(3));
        for _ in 0..3 {
            assert_eq!(private_ahead.admit_new(), PublicRowAdmission::Public);
        }
        private_ahead.reconcile(day(3), 2).unwrap();

        let mut snapshot_ahead = DailyPublicRowBudget::new(day(3));
        assert_eq!(snapshot_ahead.admit_new(), PublicRowAdmission::Public);
        snapshot_ahead.reconcile(day(3), 3).unwrap();

        assert_eq!(private_ahead.admitted(), 3);
        assert_eq!(snapshot_ahead.admitted(), 3);
    }

    #[test]
    fn reconciling_the_same_surviving_rows_is_idempotent() {
        let mut budget = DailyPublicRowBudget::new(day(3));

        budget.reconcile(day(3), 3).unwrap();
        budget.reconcile(day(3), 3).unwrap();

        assert_eq!(budget.admitted(), 3);
    }

    #[test]
    fn reconciliation_advances_the_day_before_applying_surviving_rows() {
        let mut budget = DailyPublicRowBudget::new(day(3));
        for _ in 0..MAX_PUBLIC_ROWS_PER_DAY {
            assert_eq!(budget.admit_new(), PublicRowAdmission::Public);
        }

        assert_eq!(budget.reconcile(day(4), 1), Ok(OpenDayUpdate::Advanced));

        assert_eq!(budget.admitted(), 1);
        assert_eq!(budget.admit_new(), PublicRowAdmission::Public);
    }

    #[test]
    fn reconciliation_saturates_surviving_rows_at_the_daily_limit() {
        let mut budget = DailyPublicRowBudget::new(day(3));

        budget.reconcile(day(3), u64::MAX).unwrap();

        assert_eq!(budget.admitted(), MAX_PUBLIC_ROWS_PER_DAY);
        assert_eq!(budget.admit_new(), PublicRowAdmission::Overflow);
    }
}
