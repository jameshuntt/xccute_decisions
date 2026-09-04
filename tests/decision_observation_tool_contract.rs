use xccute_decisions::{
    builtin_observation_tools, grep_observation_tool, observation_tool_by_id, pgrep_observation_tool,
};
use xccute_runtime::RuntimeObservationKind;

#[test]
fn builtin_decision_tools_cover_search_transform_and_state_checks() {
    let tool_ids: Vec<_> = builtin_observation_tools()
        .into_iter()
        .map(|tool| tool.tool_id)
        .collect();

    assert!(tool_ids.contains(&"grep.pattern_search"));
    assert!(tool_ids.contains(&"pgrep.process_search"));
    assert!(tool_ids.contains(&"sed.text_transform"));
    assert!(tool_ids.contains(&"stat.file_check"));
}

#[test]
fn grep_tool_builds_required_pattern_search_question() {
    let question = grep_observation_tool().required_question(
        "check.errors",
        "logs.grep_errors",
        "Did grep find ERROR lines?",
    );

    assert_eq!(question.logical_id, "check.errors");
    assert_eq!(question.operation_logical_id, "logs.grep_errors");
    assert_eq!(question.kind, RuntimeObservationKind::PatternSearch);
    assert!(question.responsibility.contains("pattern"));
    assert!(question.required);
}

#[test]
fn pgrep_tool_can_make_optional_process_question() {
    let question = pgrep_observation_tool().default_optional_question(
        "check.worker",
        "fleet.pgrep_worker",
    );

    assert_eq!(question.kind, RuntimeObservationKind::ProcessSearch);
    assert!(!question.required);
    assert!(question.question.contains("pgrep"));
}

#[test]
fn decision_tool_digest_is_stable_and_tool_identity_sensitive() {
    let grep_a = grep_observation_tool().digest();
    let grep_b = grep_observation_tool().digest();
    let pgrep = pgrep_observation_tool().digest();

    assert_eq!(grep_a, grep_b);
    assert_ne!(grep_a, pgrep);
}

#[test]
fn decision_tool_lookup_is_by_stable_tool_id() {
    let tool = observation_tool_by_id("grep.pattern_search").expect("grep tool");

    assert_eq!(tool.program, "grep");
    assert_eq!(tool.kind, RuntimeObservationKind::PatternSearch);
    assert!(observation_tool_by_id("missing.tool").is_none());
}

#[test]
fn grep_tool_builds_source_question_spec() {
    let spec = grep_observation_tool().required_question_spec(
        "check.errors",
        "logs.grep_errors",
        "Did grep find ERROR lines?",
    );

    assert_eq!(spec.tool_id, "grep.pattern_search");
    assert!(spec.required);
    assert_eq!(spec.operation_logical_id, "logs.grep_errors");
}
