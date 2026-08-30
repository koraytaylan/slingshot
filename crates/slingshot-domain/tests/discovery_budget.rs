//! What one discovery job may spend, and when it must stop.
//!
//! Every vector here is deliberate about a distinction the contract turns on:
//! a budget that runs out is a failure with no page, while a page that fills up
//! is a success with a token. Confusing the two would either lose matches or
//! promise a cursor that does not exist, so the fixture proves each side
//! separately and proves which one wins where they meet.
//!
//! Nothing here waits. The clock is injected, so the instant before the
//! deadline, the deadline itself, and the instant after it are all reachable
//! without spending any of them.

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::discovery_budget::{
    BOOLEAN_PROPERTY_BYTES, CancellationSignal, DISCOVERY_BUDGET_EXCEEDED, DiscoveryBudget,
    DiscoveryBudgetFailure, DiscoveryExecutionBudget, DiscoveryPage, DiscoveryStop,
    ElapsedMonotonicClock, MatchDisposition, NUMERIC_PROPERTY_BYTES, textual_property_bytes,
};
use slingshot_domain::command::result_window::{ResultLimit, ResultOffset};

/// Vectors this test reads.
const FIXTURE: &str = include_str!("fixtures/commands/discovery-budget.jsonl");

/// A clock that answers one number, forever.
struct FixedClock {
    /// Milliseconds it reports.
    elapsed: u64,
}

impl ElapsedMonotonicClock for FixedClock {
    fn elapsed_milliseconds(&self) -> u64 {
        self.elapsed
    }
}

/// A signal that answers one way, forever.
struct FixedCancellation {
    /// Whether it reports cancellation.
    cancelled: bool,
}

impl CancellationSignal for FixedCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

/// A signal that reports cancellation only after it has been asked once.
///
/// It stands for the caller who cancels while a repository call is in flight:
/// the boundary before the call sees nothing, the boundary after it sees the
/// cancellation.
struct CancelledDuringCall {
    /// How many times it has been asked.
    asked: std::cell::Cell<u64>,
}

impl CancellationSignal for CancelledDuringCall {
    fn is_cancelled(&self) -> bool {
        let asked = self.asked.get();
        self.asked.set(asked + 1);
        asked > 0
    }
}

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Reads one row's unsigned member.
fn number(row: &Value, member: &str) -> u64 {
    row[member].as_u64().unwrap_or_else(|| panic!("{member} is an unsigned integer in {row}"))
}

/// Returns every fixture row of one kind.
fn rows(kind: &str) -> Vec<Value> {
    FIXTURE
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .filter(|row| text(row, "kind") == kind)
        .collect()
}

/// Returns the budget the fixture names.
fn named_budget(name: &str) -> DiscoveryBudget {
    match name {
        "candidate_nodes" => DiscoveryBudget::CandidateNodes,
        "property_values" => DiscoveryBudget::PropertyValues,
        "property_bytes" => DiscoveryBudget::PropertyBytes,
        "criterion_evaluations" => DiscoveryBudget::CriterionEvaluations,
        "execution_duration" => DiscoveryBudget::ExecutionDuration,
        other => panic!("the fixture names a budget this contract does not have: {other}"),
    }
}

/// Spends `amount` of `budget` from a job with `remaining` left.
///
/// The budget starts full and is drawn down to `remaining` first, so every
/// vector exercises the same counter the job itself would.
fn spend(budget: &str, remaining: u64, amount: u64) -> Result<(), DiscoveryStop> {
    let mut job = DiscoveryExecutionBudget::full();
    match budget {
        "candidate_nodes" => {
            draw_down(
                &mut job,
                remaining,
                |job| job.charge_candidate_node(),
                |job| job.remaining_candidate_nodes(),
            );
            for _ in 0..amount {
                job.charge_candidate_node()?;
            }
            Ok(())
        }
        "criterion_evaluations" => {
            draw_down(
                &mut job,
                remaining,
                |job| job.charge_criterion_evaluation(),
                |job| job.remaining_criterion_evaluations(),
            );
            for _ in 0..amount {
                job.charge_criterion_evaluation()?;
            }
            Ok(())
        }
        "property_values" => spend_property_values(job, remaining, amount),
        "property_bytes" => spend_property_bytes(job, remaining, amount),
        other => panic!("the fixture names a budget this test cannot spend: {other}"),
    }
}

/// Draws one counter down to `remaining` in one charge.
fn draw_down(
    job: &mut DiscoveryExecutionBudget,
    remaining: u64,
    mut charge: impl FnMut(&mut DiscoveryExecutionBudget) -> Result<(), DiscoveryStop>,
    read: impl Fn(&DiscoveryExecutionBudget) -> u64,
) {
    let mut left = read(job);
    while left > remaining {
        charge(job).expect("drawing a full budget down stays inside it");
        left = read(job);
    }
}

/// Spends property values without letting the byte budget interfere.
fn spend_property_values(
    mut job: DiscoveryExecutionBudget,
    remaining: u64,
    amount: u64,
) -> Result<(), DiscoveryStop> {
    while job.remaining_property_values() > remaining {
        job.charge_property_value(0).expect("a valueless charge stays inside both budgets");
    }
    for _ in 0..amount {
        job.charge_property_value(0)?;
    }
    Ok(())
}

/// Spends property bytes in one charge each, so the byte bound is the one hit.
fn spend_property_bytes(
    mut job: DiscoveryExecutionBudget,
    remaining: u64,
    amount: u64,
) -> Result<(), DiscoveryStop> {
    let spent = job.remaining_property_bytes() - remaining;
    if spent > 0 {
        job.charge_property_value(spent).expect("drawing the byte budget down stays inside it");
    }
    job.charge_property_value(amount)
}

#[test]
fn every_charge_lands_where_the_fixture_says_it_does() {
    let vectors = rows("charge");
    assert!(vectors.len() >= 24, "every counter is proved at both edges");
    for row in &vectors {
        let budget = text(row, "budget");
        let outcome = spend(budget, number(row, "remaining"), number(row, "amount"));
        let note = text(row, "note");
        match (row["accepted"].as_bool(), outcome) {
            (Some(true), Ok(())) => (),
            (Some(false), Err(DiscoveryStop::BudgetExceeded(exceeded))) => {
                assert_eq!(exceeded, named_budget(budget), "{note}: the wrong budget was blamed");
            }
            (Some(true), Err(stop)) => panic!("{note}: refused as {stop:?}"),
            (Some(false), Ok(())) => panic!("{note}: accepted"),
            (_, Err(stop)) => panic!("{note}: stopped as {stop:?}"),
            (None, _) => panic!("{note}: the fixture states whether it is accepted"),
        }
    }
}

#[test]
fn a_charge_larger_than_the_whole_budget_never_wraps_it() {
    let mut job = DiscoveryExecutionBudget::full();
    let before = job.remaining_property_bytes();
    assert_eq!(
        job.charge_property_value(u64::MAX),
        Err(DiscoveryStop::BudgetExceeded(DiscoveryBudget::PropertyBytes))
    );
    assert!(
        job.remaining_property_bytes() <= before,
        "a refused charge leaves no more budget than it found"
    );
}

#[test]
fn every_boundary_checks_cancellation_first_and_time_second() {
    let deadline = DiscoveryExecutionBudget::full().execution_deadline_milliseconds();
    let vectors = rows("boundary");
    assert!(vectors.len() >= 7, "both orderings are proved");
    for row in &vectors {
        let job = DiscoveryExecutionBudget::full();
        let cancellation =
            FixedCancellation { cancelled: row["cancelled"].as_bool().expect("a Boolean") };
        let clock = FixedClock { elapsed: number(row, "elapsed") };
        let note = text(row, "note");
        match (text(row, "outcome"), job.observe_boundary(&cancellation, &clock)) {
            ("proceed", Ok(())) => (),
            ("cancelled", Err(DiscoveryStop::Cancelled)) => (),
            (
                "execution_duration",
                Err(DiscoveryStop::BudgetExceeded(DiscoveryBudget::ExecutionDuration)),
            ) => (),
            (expected, actual) => panic!("{note}: expected {expected}, got {actual:?}"),
        }
        assert_eq!(
            job.execution_deadline_milliseconds(),
            deadline,
            "the deadline is the manifest's"
        );
    }
}

#[test]
fn a_call_that_returns_at_or_after_the_deadline_is_never_interpreted() {
    let job = DiscoveryExecutionBudget::full();
    for row in &rows("call_return") {
        let clock = FixedClock { elapsed: number(row, "elapsed") };
        let note = text(row, "note");
        match (text(row, "outcome"), job.observe_call_return(&clock)) {
            ("interpreted", Ok(())) => (),
            (
                "execution_duration",
                Err(DiscoveryStop::BudgetExceeded(DiscoveryBudget::ExecutionDuration)),
            ) => (),
            (expected, actual) => panic!("{note}: expected {expected}, got {actual:?}"),
        }
    }
}

#[test]
fn cancellation_observed_after_a_call_returns_stops_the_job_there() {
    let job = DiscoveryExecutionBudget::full();
    let cancellation = CancelledDuringCall { asked: std::cell::Cell::new(0) };
    let clock = FixedClock { elapsed: 0 };
    assert_eq!(
        job.observe_boundary(&cancellation, &clock),
        Ok(()),
        "the boundary before the call sees nothing"
    );
    assert_eq!(
        job.observe_boundary(&cancellation, &clock),
        Err(DiscoveryStop::Cancelled),
        "the boundary after it sees the cancellation"
    );
    assert_eq!(
        job.observe_boundary(&cancellation, &clock),
        Err(DiscoveryStop::Cancelled),
        "and every later boundary agrees, so no further call is made"
    );
    assert_eq!(DiscoveryStop::Cancelled.budget(), None, "cancellation is not a budget");
}

#[test]
fn every_property_value_charges_what_its_repository_type_costs() {
    for row in &rows("property_bytes") {
        let expected = number(row, "bytes");
        let note = text(row, "note");
        let charged = match text(row, "repository_type") {
            "string" => textual_property_bytes(text(row, "spelling")),
            "long" | "double" => NUMERIC_PROPERTY_BYTES,
            "boolean" => BOOLEAN_PROPERTY_BYTES,
            "binary" => expected,
            other => panic!("{note}: an unknown repository type {other}"),
        };
        assert_eq!(charged, expected, "{note}");
    }
    assert_eq!(NUMERIC_PROPERTY_BYTES, 8, "the repository stores both in eight bytes");
    assert_eq!(BOOLEAN_PROPERTY_BYTES, 1);
}

#[test]
fn every_match_is_disposed_of_in_the_order_the_contract_states() {
    let vectors = rows("disposition");
    assert!(vectors.len() >= 9, "each of the three outcomes is proved more than once");
    for row in &vectors {
        let mut page = page_with(
            number(row, "remaining_offset"),
            number(row, "remaining_limit"),
            number(row, "remaining_result_bytes"),
        );
        let bytes_before = page.remaining_result_bytes();
        let admitted_before = page.admitted();
        let disposition = page.dispose(number(row, "match_bytes"));
        let note = text(row, "note");
        match (text(row, "disposition"), disposition) {
            ("skipped_for_offset", MatchDisposition::SkippedForOffset) => {
                assert_eq!(
                    page.remaining_result_bytes(),
                    bytes_before,
                    "{note}: a skipped match charges no result bytes"
                );
                assert_eq!(
                    page.admitted(),
                    admitted_before,
                    "{note}: a skipped match is not in the page"
                );
            }
            ("admitted", MatchDisposition::Admitted) => assert_eq!(
                page.admitted(),
                admitted_before + 1,
                "{note}: an admitted match is in the page"
            ),
            ("page_completed", MatchDisposition::PageCompleted) => {
                assert_eq!(
                    page.admitted(),
                    admitted_before,
                    "{note}: a completed page does not carry the match it stopped before"
                );
                assert_eq!(
                    page.remaining_result_bytes(),
                    bytes_before,
                    "{note}: nor does it charge for it"
                );
            }
            (expected, actual) => panic!("{note}: expected {expected}, got {actual:?}"),
        }
    }
}

/// Returns a page drawn down to the stated remainders.
///
/// A page whose offset is not yet spent has an untouched limit and an untouched
/// byte budget, because the offset is always spent first - so that combination
/// is built directly, and every other combination is reached by admitting one
/// match of exactly the excess.
fn page_with(offset: u64, limit: u64, result_bytes: u64) -> DiscoveryPage {
    /// One extra match, admitted to draw the page down to where it is wanted.
    const DRAWDOWN_MATCH: u64 = 1;

    let offset = ResultOffset::new(offset).expect("the fixture stays inside the offset bound");
    let full = DiscoveryPage::resuming(ResultLimit::new(1).expect("a legal limit"))
        .remaining_result_bytes();
    let excess = full - result_bytes;
    if offset.count() > 0 {
        assert_eq!(excess, 0, "an unspent offset means an unspent byte budget");
        return DiscoveryPage::beginning(
            offset,
            ResultLimit::new(limit).expect("the fixture stays inside the limit bound"),
        );
    }
    let mut page = DiscoveryPage::beginning(
        offset,
        ResultLimit::new(limit + DRAWDOWN_MATCH).expect("the fixture leaves room to draw down"),
    );
    assert_eq!(page.dispose(excess), MatchDisposition::Admitted, "drawing the page down");
    page
}

#[test]
fn a_resumed_page_never_skips_again() {
    /// Matches the resumed page may carry.
    const RESUMED_LIMIT: u64 = 10;

    let page = DiscoveryPage::resuming(ResultLimit::new(RESUMED_LIMIT).expect("a legal limit"));
    assert_eq!(
        page.remaining_offset(),
        0,
        "the token resumes after the last emitted match, so skipping again would skip \
         content the caller has not seen"
    );
    assert_eq!(page.admitted(), 0);
}

#[test]
fn a_full_page_completes_before_another_repository_charge() {
    /// Matches this page may carry, chosen so the page fills while matches
    /// remain to be offered.
    const PAGE_LIMIT: u64 = 2;
    /// Bytes each of those matches canonicalizes to.
    const MATCH_BYTES: u64 = 16;

    let mut page = DiscoveryPage::beginning(
        ResultOffset::beginning(),
        ResultLimit::new(PAGE_LIMIT).expect("a legal limit"),
    );
    assert_eq!(page.dispose(MATCH_BYTES), MatchDisposition::Admitted);
    assert!(!page.is_complete());
    assert_eq!(page.dispose(MATCH_BYTES), MatchDisposition::Admitted);
    assert!(page.is_complete(), "the page is full the instant its last match is admitted");
    assert_eq!(page.dispose(MATCH_BYTES), MatchDisposition::PageCompleted);
    assert_eq!(page.admitted(), PAGE_LIMIT, "nothing was admitted past the limit");
}

#[test]
fn every_failure_renders_as_the_closed_object_the_contract_declares() {
    let vectors = rows("failure");
    let declared: Vec<&str> = vectors.iter().map(|row| text(row, "budget")).collect();
    assert_eq!(
        declared,
        vec![
            "candidate_nodes",
            "property_values",
            "property_bytes",
            "criterion_evaluations",
            "execution_duration",
        ],
        "five literals, and result bytes is not one of them"
    );
    for row in &vectors {
        let failure = DiscoveryBudgetFailure::new(named_budget(text(row, "budget")));
        assert_eq!(failure.failure, DISCOVERY_BUDGET_EXCEEDED);
        assert_eq!(
            serde_json::to_string(&failure).expect("a failure serializes"),
            text(row, "rendering"),
            "{}",
            text(row, "note")
        );
    }
}

#[test]
fn a_full_budget_is_exactly_what_the_manifest_declares() {
    let contract = CommandContract::embedded();
    let job = DiscoveryExecutionBudget::full();
    assert_eq!(
        job.remaining_candidate_nodes(),
        contract.limit("maximum_discovery_candidate_nodes")
    );
    assert_eq!(
        job.remaining_property_values(),
        contract.limit("maximum_discovery_property_values")
    );
    assert_eq!(job.remaining_property_bytes(), contract.limit("maximum_discovery_property_bytes"));
    assert_eq!(
        job.remaining_criterion_evaluations(),
        contract.limit("maximum_discovery_criterion_evaluations")
    );
    assert_eq!(
        job.execution_deadline_milliseconds(),
        contract.limit("maximum_discovery_execution_duration_milliseconds")
    );
}
