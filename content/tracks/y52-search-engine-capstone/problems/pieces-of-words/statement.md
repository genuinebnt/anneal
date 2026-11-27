**Capstone, step 4 of 7.** People also search for parts of words: `*cast*` should find `broadcast` and `Typecast`. Reading every page for every question is too slow.

The starter holds the reference code of steps 1 to 3. You will need to change what a batch keeps.

**Spoiler warning.** That code solves the earlier steps.

Write `find_pattern(pattern)`.

**What fits.** Fold the pattern and each page's whole text with `fold`. Split the folded pattern at `*`, and drop the empty pieces. 

A page **fits** when the pieces stand in its folded text **in this order, without sharing bytes**. They may stand anywhere, across words and punctuation. A pattern with no pieces, such as `""` or `**`, fits every page.

**Candidates.** A **window** is three neighbouring **bytes**. A page is a **candidate** when its folded text has every window of every piece, anywhere in the text. A piece shorter than three bytes has no window. If the pattern has no window at all, every page is a candidate.

The answer holds the number of candidates over all batches, and the fitting pages in rising order. Only candidates should need reading.

**Speed is not graded.** The tests check the candidate count, but cannot see which pages you read. To measure the gain, compare your search with reading every page, for a rare pattern over 20 000 pages.
