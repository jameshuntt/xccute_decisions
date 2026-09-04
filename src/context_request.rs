//! Focused context requests for the next decision step.
//!
//! `DecisionContextPack` answers "what compact context is allowed to enter the
//! next decision?" A context request answers the next question: "which focused
//! observation questions should be asked from that compact context?" This keeps
//! the system adaptive without turning the context window into a raw log dump.

use crate::context_pack::DecisionContextPack;
use crate::guide::DecisionQuestionSpec;
use xccute_runtime::{RuntimeObservationRequirement, StableDigest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionContextRequestSelection {
    RequiredOnly,
    First(usize),
    ByLogicalIds(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionContextRequestError {
    NoActiveQuestionsInContextPack,
    EmptyQuestionSelection,
    QuestionNotInContext { logical_id: String },
    DuplicateQuestionSelection { logical_id: String },
    MinimumObservationCountUnsatisfied {
        minimum_required: usize,
        selected_count: usize,
    },
}

pub type DecisionContextRequestResult<T> = Result<T, DecisionContextRequestError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionContextRequest {
    pub request_id: String,
    pub goal: String,
    pub context_pack_digest: StableDigest,
    pub journal_digest: StableDigest,
    pub active_runbook_contract_digest: Option<StableDigest>,
    pub selected_questions: Vec<DecisionQuestionSpec>,
    pub carried_replay_summaries: Vec<String>,
    pub minimum_observation_count: usize,
    pub context_budget_bytes: usize,
}

impl DecisionContextRequest {
    pub fn from_context_pack(
        request_id: impl Into<String>,
        goal: impl Into<String>,
        pack: &DecisionContextPack,
        selection: DecisionContextRequestSelection,
        minimum_observation_count: usize,
    ) -> DecisionContextRequestResult<Self> {
        if pack.active_questions.is_empty() {
            return Err(DecisionContextRequestError::NoActiveQuestionsInContextPack);
        }

        let selected_questions = select_questions(pack, selection)?;
        if selected_questions.is_empty() {
            return Err(DecisionContextRequestError::EmptyQuestionSelection);
        }

        if selected_questions.len() < minimum_observation_count {
            return Err(DecisionContextRequestError::MinimumObservationCountUnsatisfied {
                minimum_required: minimum_observation_count,
                selected_count: selected_questions.len(),
            });
        }

        Ok(Self {
            request_id: request_id.into(),
            goal: goal.into(),
            context_pack_digest: pack.digest(),
            journal_digest: pack.journal_digest.clone(),
            active_runbook_contract_digest: pack.active_runbook_contract_digest.clone(),
            selected_questions,
            carried_replay_summaries: pack
                .selected_replay_steps
                .iter()
                .map(|step| step.compact_summary.clone())
                .collect(),
            minimum_observation_count,
            context_budget_bytes: pack.context_budget_bytes,
        })
    }

    pub fn required_from_context_pack(
        request_id: impl Into<String>,
        goal: impl Into<String>,
        pack: &DecisionContextPack,
    ) -> DecisionContextRequestResult<Self> {
        Self::from_context_pack(
            request_id,
            goal,
            pack,
            DecisionContextRequestSelection::RequiredOnly,
            1,
        )
    }

    pub fn selected_context_budget_hint(&self) -> usize {
        self.selected_questions
            .iter()
            .map(|question| question.context_budget_hint)
            .sum()
    }

    pub fn selected_question_ids(&self) -> Vec<String> {
        self.selected_questions
            .iter()
            .map(|question| question.logical_id.clone())
            .collect()
    }

    pub fn compact_request(&self) -> String {
        let mut request = String::new();
        request.push_str("goal: ");
        request.push_str(&self.goal);
        request.push('\n');
        if !self.carried_replay_summaries.is_empty() {
            request.push_str("carried_replay:\n");
            for summary in &self.carried_replay_summaries {
                request.push_str("- ");
                request.push_str(summary);
                request.push('\n');
            }
        }
        request.push_str("next_questions:\n");
        for question in &self.selected_questions {
            let required = if question.required { "required" } else { "optional" };
            request.push_str("- ");
            request.push_str(required);
            request.push(' ');
            request.push_str(&question.logical_id);
            request.push_str(" via ");
            request.push_str(&question.tool_id);
            request.push_str(" op ");
            request.push_str(&question.operation_logical_id);
            request.push_str(": ");
            request.push_str(&question.question);
            request.push('\n');
        }
        request
    }

    pub fn to_observation_requirements(&self) -> Vec<RuntimeObservationRequirement> {
        self.selected_questions
            .iter()
            .map(|question| question.materialize().to_observation_requirement())
            .collect()
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.context.request.v1\n");
        push_stable_field(&mut material, "request_id", &self.request_id);
        push_stable_field(&mut material, "goal", &self.goal);
        push_stable_field(
            &mut material,
            "context_pack_digest",
            self.context_pack_digest.as_str(),
        );
        push_stable_field(&mut material, "journal_digest", self.journal_digest.as_str());
        push_stable_field(
            &mut material,
            "active_runbook_contract_digest",
            self.active_runbook_contract_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "minimum_observation_count",
            &self.minimum_observation_count.to_string(),
        );
        push_stable_field(
            &mut material,
            "context_budget_bytes",
            &self.context_budget_bytes.to_string(),
        );
        for (index, summary) in self.carried_replay_summaries.iter().enumerate() {
            material.push_str("carried_replay_summary[");
            material.push_str(&index.to_string());
            material.push_str("]\n");
            push_stable_field(&mut material, "summary", summary);
        }
        for (index, question) in self.selected_questions.iter().enumerate() {
            material.push_str("selected_question[");
            material.push_str(&index.to_string());
            material.push_str("]\n");
            push_stable_field(&mut material, "question.digest", question.digest().as_str());
            material.push_str(&question.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

fn select_questions(
    pack: &DecisionContextPack,
    selection: DecisionContextRequestSelection,
) -> DecisionContextRequestResult<Vec<DecisionQuestionSpec>> {
    match selection {
        DecisionContextRequestSelection::RequiredOnly => Ok(pack
            .active_questions
            .iter()
            .filter(|question| question.required)
            .cloned()
            .collect()),
        DecisionContextRequestSelection::First(count) => {
            if count == 0 {
                return Err(DecisionContextRequestError::EmptyQuestionSelection);
            }
            Ok(pack.active_questions.iter().take(count).cloned().collect())
        }
        DecisionContextRequestSelection::ByLogicalIds(ids) => select_by_logical_ids(pack, ids),
    }
}

fn select_by_logical_ids(
    pack: &DecisionContextPack,
    ids: Vec<String>,
) -> DecisionContextRequestResult<Vec<DecisionQuestionSpec>> {
    if ids.is_empty() {
        return Err(DecisionContextRequestError::EmptyQuestionSelection);
    }

    let mut selected = Vec::new();
    let mut seen = Vec::<String>::new();
    for id in ids {
        if seen.iter().any(|existing| existing == &id) {
            return Err(DecisionContextRequestError::DuplicateQuestionSelection { logical_id: id });
        }
        seen.push(id.clone());

        let question = pack
            .active_questions
            .iter()
            .find(|question| question.logical_id == id)
            .cloned()
            .ok_or(DecisionContextRequestError::QuestionNotInContext { logical_id: id })?;
        selected.push(question);
    }

    Ok(selected)
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
