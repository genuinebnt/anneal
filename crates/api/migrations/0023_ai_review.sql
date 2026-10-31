-- AI help (crates/api/src/ai.rs): the interviewer's suggested marks for a system design attempt, kept beside the owner's own marks.
-- A new nullable column only; nothing existing changes.
ALTER TABLE sd_attempts ADD COLUMN ai_review jsonb;
