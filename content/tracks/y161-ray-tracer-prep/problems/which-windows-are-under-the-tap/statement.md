A drawing program has thousands of windows on a huge canvas. When the user taps, drags a line or draws a selection, it must find the windows concerned, without looking at all of them.

`Index::new(rects)` prepares. `search(touches)` returns the numbers of all rectangles that the question `touches` says yes to, **in rising order**.

## The question

You are not told what the search is for. It may be a tap, a stripe, a selection box or something else. All you can do is call `touches(&rectangle)`.

- You may ask about **any** rectangle, whether it is in the list or not.
- The question is **honest about containment**: if it says yes to a rectangle, it also says yes to every rectangle that contains it.
- Report number `i` only if the question says yes to `rects[i]` itself.

Rectangles have `x0 <= x1` and `y0 <= y1`. Edges are included.

A rectangle may have NaN coordinates. The question never says yes to it. It must not hide other rectangles or cause a panic.

## How little to ask

The tests count your questions.

| Rectangles | Question | Questions allowed |
|---|---|---|
| 4,096 small tiles in a square grid, listed in any order | a tap | 100 |
| the same | a stripe across the whole grid, either way, touching 64 tiles | 700 |
| the same | something far from everything | 4 |
| the same | everything passes | 12,288 |
| 4,096 long bars stacked side by side | a tap | 100 |

Some tests use 30,000 rectangles in exactly the same place.
