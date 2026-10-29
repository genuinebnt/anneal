-- Why an attempt went badly, picked by the owner after logging Attempted or With help (optional, one click): the idea,
-- the implementation, the edge cases or the complexity. NULL for every existing attempt.
ALTER TABLE attempts ADD COLUMN why text CHECK (why IN ('idea', 'implementation', 'edge_cases', 'complexity'));
