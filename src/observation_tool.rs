//! Observation tool responsibilities for guided decisions.
//!
//! These are not command builders. They are decision-facing contracts that say
//! what a command family is allowed to contribute to a guided decision path. For
//! example, `grep` can answer pattern-search questions, while `pgrep` can answer
//! process-search questions.

use crate::guide::DecisionQuestionSpec;
use xccute_runtime::{RuntimeDecisionQuestion, RuntimeObservationKind, StableDigest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionObservationTool {
    pub tool_id: &'static str,
    pub program: &'static str,
    pub kind: RuntimeObservationKind,
    pub responsibility: &'static str,
    pub default_question: &'static str,
    pub compact_fact_shape: &'static str,
    pub success_meaning: &'static str,
    pub empty_meaning: &'static str,
}

impl DecisionObservationTool {
    pub fn required_question(
        &self,
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        question: impl Into<String>,
    ) -> RuntimeDecisionQuestion {
        RuntimeDecisionQuestion::required(
            logical_id,
            operation_logical_id,
            self.kind.clone(),
            question,
            self.responsibility,
        )
    }

    pub fn optional_question(
        &self,
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        question: impl Into<String>,
    ) -> RuntimeDecisionQuestion {
        RuntimeDecisionQuestion::optional(
            logical_id,
            operation_logical_id,
            self.kind.clone(),
            question,
            self.responsibility,
        )
    }

    pub fn default_required_question(
        &self,
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
    ) -> RuntimeDecisionQuestion {
        self.required_question(logical_id, operation_logical_id, self.default_question)
    }

    pub fn default_optional_question(
        &self,
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
    ) -> RuntimeDecisionQuestion {
        self.optional_question(logical_id, operation_logical_id, self.default_question)
    }

    pub fn required_question_spec(
        &self,
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        question: impl Into<String>,
    ) -> DecisionQuestionSpec {
        DecisionQuestionSpec::required(logical_id, operation_logical_id, self, question)
    }

    pub fn optional_question_spec(
        &self,
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        question: impl Into<String>,
    ) -> DecisionQuestionSpec {
        DecisionQuestionSpec::optional(logical_id, operation_logical_id, self, question)
    }

    pub fn default_required_question_spec(
        &self,
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
    ) -> DecisionQuestionSpec {
        self.required_question_spec(logical_id, operation_logical_id, self.default_question)
    }

    pub fn default_optional_question_spec(
        &self,
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
    ) -> DecisionQuestionSpec {
        self.optional_question_spec(logical_id, operation_logical_id, self.default_question)
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.observation.tool.v1\n");
        push_stable_field(&mut material, "tool_id", self.tool_id);
        push_stable_field(&mut material, "program", self.program);
        push_stable_field(&mut material, "kind", &self.kind.stable_label());
        push_stable_field(&mut material, "responsibility", self.responsibility);
        push_stable_field(&mut material, "default_question", self.default_question);
        push_stable_field(&mut material, "compact_fact_shape", self.compact_fact_shape);
        push_stable_field(&mut material, "success_meaning", self.success_meaning);
        push_stable_field(&mut material, "empty_meaning", self.empty_meaning);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

pub fn grep_observation_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "grep.pattern_search",
        program: "grep",
        kind: RuntimeObservationKind::PatternSearch,
        responsibility: "answer whether text contains a requested pattern while keeping raw output behind a digest",
        default_question: "Did grep find the requested pattern?",
        compact_fact_shape: "found:boolean, collected_count:number, compact_summary:string, output_digest:sha256",
        success_meaning: "matches were found",
        empty_meaning: "no matches were found; this can still be a successful observation",
    }
}

pub fn pgrep_observation_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "pgrep.process_search",
        program: "pgrep",
        kind: RuntimeObservationKind::ProcessSearch,
        responsibility: "answer whether a process matching a declared pattern is running",
        default_question: "Did pgrep find a matching process?",
        compact_fact_shape: "found:boolean, collected_count:number, compact_summary:string, output_digest:sha256",
        success_meaning: "one or more process ids matched",
        empty_meaning: "no process ids matched",
    }
}

pub fn sed_observation_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "sed.text_transform",
        program: "sed",
        kind: RuntimeObservationKind::TextTransform,
        responsibility: "normalize or extract a small text projection for decision context without storing full input",
        default_question: "Did sed produce the expected transformed text?",
        compact_fact_shape: "found:boolean, collected_count:number, compact_summary:string, output_digest:sha256",
        success_meaning: "the transform produced decision-relevant output",
        empty_meaning: "the transform produced no decision-relevant output",
    }
}

pub fn awk_observation_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "awk.structured_text_projection",
        program: "awk",
        kind: RuntimeObservationKind::TextTransform,
        responsibility: "project structured fields from text into a compact decision fact",
        default_question: "Did awk collect the expected structured fields?",
        compact_fact_shape: "found:boolean, collected_count:number, compact_summary:string, output_digest:sha256",
        success_meaning: "structured fields were collected",
        empty_meaning: "no structured fields were collected",
    }
}

pub fn stat_observation_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "stat.file_check",
        program: "stat",
        kind: RuntimeObservationKind::FileCheck,
        responsibility: "answer whether a filesystem target exists and summarize selected metadata",
        default_question: "Did stat observe the expected filesystem target?",
        compact_fact_shape: "found:boolean, collected_count:number, compact_summary:string, output_digest:sha256",
        success_meaning: "target metadata was observed",
        empty_meaning: "target metadata was not observed",
    }
}

pub fn find_observation_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "find.file_discovery",
        program: "find",
        kind: RuntimeObservationKind::FileCheck,
        responsibility: "discover whether filesystem paths matching a bounded query exist",
        default_question: "Did find discover matching filesystem paths?",
        compact_fact_shape: "found:boolean, collected_count:number, compact_summary:string, output_digest:sha256",
        success_meaning: "matching paths were discovered",
        empty_meaning: "no matching paths were discovered",
    }
}

pub fn ps_observation_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "ps.process_snapshot",
        program: "ps",
        kind: RuntimeObservationKind::ProcessSearch,
        responsibility: "summarize a bounded process snapshot for decision context",
        default_question: "Did ps show the expected process state?",
        compact_fact_shape: "found:boolean, collected_count:number, compact_summary:string, output_digest:sha256",
        success_meaning: "process state was observed",
        empty_meaning: "requested process state was not observed",
    }
}

pub fn builtin_observation_tools() -> Vec<DecisionObservationTool> {
    vec![
        grep_observation_tool(),
        pgrep_observation_tool(),
        sed_observation_tool(),
        awk_observation_tool(),
        stat_observation_tool(),
        find_observation_tool(),
        ps_observation_tool(),
    ]
}

pub fn observation_tool_by_id(tool_id: &str) -> Option<DecisionObservationTool> {
    builtin_observation_tools()
        .into_iter()
        .find(|tool| tool.tool_id == tool_id)
}

fn push_stable_field(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(".len=");
    out.push_str(&value.len().to_string());
    out.push('\n');
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}
