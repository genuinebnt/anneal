-- A repeat done for practice (docs/DSA.md, decision 35): logged like any attempt and counted in the streak and the
-- solve record, but it never schedules a review or changes where a problem stands.
ALTER TABLE attempts DROP CONSTRAINT attempts_kind_check;
ALTER TABLE attempts ADD CONSTRAINT attempts_kind_check CHECK (kind IN ('practice', 'resolve', 'mock', 'extra'));
