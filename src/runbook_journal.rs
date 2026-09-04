//! Append-only journals for decision runbook records.
//!
//! A `DecisionRunbookRecord` proves one runbook instance was satisfied. This
//! module links many records into a deterministic journal so a longer solution
//! path can be replayed without loading every raw command output into context.
//! Each entry keeps a compact summary and links to the previous entry digest.

use crate::runbook::DecisionRunbookMode;
use crate::runbook_record::DecisionRunbookRecord;
use xccute_runtime::StableDigest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionRunbookJournalError {
    EntryIndexMismatch {
        expected_index: usize,
        actual_index: usize,
    },
    PreviousEntryDigestMismatch {
        index: usize,
        expected_previous_entry_digest: Option<StableDigest>,
        actual_previous_entry_digest: Option<StableDigest>,
    },
    DuplicateRecordDigest {
        first_index: usize,
        duplicate_index: usize,
        record_digest: StableDigest,
    },
}

pub type DecisionRunbookJournalResult<T> = Result<T, DecisionRunbookJournalError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRunbookJournalEntry {
    pub index: usize,
    pub previous_entry_digest: Option<StableDigest>,
    pub runbook_id: String,
    pub mode: DecisionRunbookMode,
    pub record_digest: StableDigest,
    pub contract_digest: StableDigest,
    pub guided_context_digest: StableDigest,
    pub acknowledged_path_digest: StableDigest,
    pub execution_receipt_digest: Option<StableDigest>,
    pub observation_receipt_digest: Option<StableDigest>,
    pub compact_summary: String,
}

impl DecisionRunbookJournalEntry {
    pub fn new(
        index: usize,
        previous_entry_digest: Option<StableDigest>,
        record: &DecisionRunbookRecord,
        compact_summary: impl Into<String>,
    ) -> Self {
        Self {
            index,
            previous_entry_digest,
            runbook_id: record.runbook_id.clone(),
            mode: record.mode.clone(),
            record_digest: record.digest(),
            contract_digest: record.contract_digest.clone(),
            guided_context_digest: record.guided_context_digest.clone(),
            acknowledged_path_digest: record.acknowledged_path_digest.clone(),
            execution_receipt_digest: record.execution_receipt_digest.clone(),
            observation_receipt_digest: record.observation_receipt_digest.clone(),
            compact_summary: compact_summary.into(),
        }
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.runbook.journal.entry.v1\n");
        push_stable_field(&mut material, "index", &self.index.to_string());
        push_stable_field(
            &mut material,
            "previous_entry_digest",
            self.previous_entry_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(&mut material, "runbook_id", &self.runbook_id);
        push_stable_field(&mut material, "mode", self.mode.stable_label());
        push_stable_field(&mut material, "record_digest", self.record_digest.as_str());
        push_stable_field(&mut material, "contract_digest", self.contract_digest.as_str());
        push_stable_field(
            &mut material,
            "guided_context_digest",
            self.guided_context_digest.as_str(),
        );
        push_stable_field(
            &mut material,
            "acknowledged_path_digest",
            self.acknowledged_path_digest.as_str(),
        );
        push_stable_field(
            &mut material,
            "execution_receipt_digest",
            self.execution_receipt_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "observation_receipt_digest",
            self.observation_receipt_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(&mut material, "compact_summary", &self.compact_summary);
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRunbookReplayStep {
    pub index: usize,
    pub runbook_id: String,
    pub mode: DecisionRunbookMode,
    pub record_digest: StableDigest,
    pub compact_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRunbookJournal {
    pub journal_id: String,
    pub entries: Vec<DecisionRunbookJournalEntry>,
}

impl DecisionRunbookJournal {
    pub fn new(journal_id: impl Into<String>) -> Self {
        Self {
            journal_id: journal_id.into(),
            entries: Vec::new(),
        }
    }

    pub fn append(
        mut self,
        record: &DecisionRunbookRecord,
        compact_summary: impl Into<String>,
    ) -> Self {
        let previous_entry_digest = self.entries.last().map(DecisionRunbookJournalEntry::digest);
        let entry = DecisionRunbookJournalEntry::new(
            self.entries.len(),
            previous_entry_digest,
            record,
            compact_summary,
        );
        self.entries.push(entry);
        self
    }

    pub fn try_from_entries(
        journal_id: impl Into<String>,
        entries: Vec<DecisionRunbookJournalEntry>,
    ) -> DecisionRunbookJournalResult<Self> {
        validate_entries(&entries)?;
        Ok(Self {
            journal_id: journal_id.into(),
            entries,
        })
    }

    pub fn replay_steps(&self) -> Vec<DecisionRunbookReplayStep> {
        self.entries
            .iter()
            .map(|entry| DecisionRunbookReplayStep {
                index: entry.index,
                runbook_id: entry.runbook_id.clone(),
                mode: entry.mode.clone(),
                record_digest: entry.record_digest.clone(),
                compact_summary: entry.compact_summary.clone(),
            })
            .collect()
    }

    pub fn compact_context(&self) -> String {
        let mut context = String::new();
        for step in self.replay_steps() {
            context.push_str(&step.index.to_string());
            context.push_str(". ");
            context.push_str(&step.runbook_id);
            context.push_str(" [");
            context.push_str(step.mode.stable_label());
            context.push_str("]: ");
            context.push_str(&step.compact_summary);
            context.push('\n');
        }
        context
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.runbook.journal.v1\n");
        push_stable_field(&mut material, "journal_id", &self.journal_id);
        for (index, entry) in self.entries.iter().enumerate() {
            material.push_str("entry[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(entry.digest().as_str());
            material.push('\n');
            material.push_str(&entry.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

fn validate_entries(entries: &[DecisionRunbookJournalEntry]) -> DecisionRunbookJournalResult<()> {
    for (expected_index, entry) in entries.iter().enumerate() {
        if entry.index != expected_index {
            return Err(DecisionRunbookJournalError::EntryIndexMismatch {
                expected_index,
                actual_index: entry.index,
            });
        }

        let expected_previous_entry_digest = if expected_index == 0 {
            None
        } else {
            Some(entries[expected_index - 1].digest())
        };

        if entry.previous_entry_digest != expected_previous_entry_digest {
            return Err(DecisionRunbookJournalError::PreviousEntryDigestMismatch {
                index: expected_index,
                expected_previous_entry_digest,
                actual_previous_entry_digest: entry.previous_entry_digest.clone(),
            });
        }

        for (first_index, prior) in entries[..expected_index].iter().enumerate() {
            if prior.record_digest == entry.record_digest {
                return Err(DecisionRunbookJournalError::DuplicateRecordDigest {
                    first_index,
                    duplicate_index: expected_index,
                    record_digest: entry.record_digest.clone(),
                });
            }
        }
    }

    Ok(())
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
