Return the most frequent word in `paragraph` that isn't in `banned`. Words are runs of ASCII
letters, compared case-insensitively and returned in lowercase. `banned` holds lowercase words.
Ties go to the alphabetically first word; return `""` if there are no words.
