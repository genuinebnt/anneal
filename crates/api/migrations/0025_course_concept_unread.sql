-- Unmarking a concept as read used to delete its row, losing when it was first read. Now the row stays and is marked unread; marking it
-- read again clears the mark. Additive: a new nullable column, no row is changed.
ALTER TABLE course_concept_state ADD COLUMN unread_at timestamptz;
