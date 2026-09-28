# cuesheet

Draws a distributed run as a **space-time diagram**: a message is a segment from the instant it
left to the instant it landed, so its slope is its flight time.

A *cue sheet* is the stage manager's document — every cue in order, with its timing and its
trigger. This is the same thing for a distributed run, and it pairs with
[cuelight](https://github.com/OlivierLmr/cuelight), which is the simulator that produces the runs.

```sh
cuesheet render run.st --style slides.sts --out run.svg
```

Two files feed it. A **document** (`.st`) states facts — who took part, and what happened when. A
**style sheet** (`.sts`) says what the names in that document *are*, and how they look. The
language ships no vocabulary of its own: there is no `crash` keyword, no `pause`, no `gst`. Two
structures, two flags and two glyphs, and every name in a document is one you declared.
