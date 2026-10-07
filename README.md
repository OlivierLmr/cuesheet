# cuesheet

Draws a distributed run as a **space-time diagram**: a message is a segment from the instant it
left to the instant it landed, so its slope is its flight time. A Mermaid sequence diagram draws
that same message as a horizontal arrow, which is a lie about every run that has a network in it.

A *cue sheet* is the stage manager's document — every cue in order, with its timing and its
trigger. This is the same thing for a distributed run, and it pairs with
[cuelight](https://github.com/OlivierLmr/cuelight), the simulator that produces the runs.

```sh
cuesheet render run.cuesheet --style course.cuestyle --open   # writes and shows run.html
cuesheet render run.cuesheet --out run.svg                    # a cuesheet.cuestyle beside it
cuelight viz --journal store/latest/journal.jsonl --format cuesheet   # writes messages.cuesheet
```

`--style` is required, because the built-in defaults know how to draw a lifeline and an arrow but
nothing about *your* words: without a sheet every kind comes out as a bare dot carrying its own
name. The one way round naming it is to put a `cuesheet.cuestyle` **beside** the document, which is
then found on its own. Only beside it — searching up the directory tree would make which sheet
applied depend on where the file happened to sit.

## Two files

A **document** (`.cuesheet`) states facts — who took part, and what happened when:

```
title "A relay that never happened"
participants n0 n1 n2 n3

  0  n0 asked do_broadcast
  0  n0 -> n1 .rb +10
100  n0 crash
159  n0 -> n2 .rb +20
377  run end quiescent
```

A **style sheet** (`.cuestyle`) says what the names in that document *are*, and how they look:

```
kind  point crash kills
kind  span  enter_cs exit_cs

style arrow        color=#14171c
style arrow:died   color=#cc79a7
style .rb          label="RB"
```

Note line `159`: it is written as an ordinary arrow. **Nobody says it failed.** `crash` is declared
as a `kills`, so the renderer works out that n0 was not taking part when that message was due out,
and draws it as a stub that never left.

## Nothing is built in

There is no `crash` keyword, no `pause`, no `gst`, no `end`. The language ships **two structures,
two flags and two glyphs**, and every name in a document is one you declared:

| | |
|---|---|
| `point` | something that happens at an instant |
| `span`  | something that lasts — closed by an instant, or by a second named event |
| `kills` · `revives` | flags on a point: its subject stops, or resumes, taking part |
| `->` · `-x` | arrived, and eaten by the network |

The subject decides *where* a thing is drawn and the category decides *how long* it lasts, and the
two are independent — which is why there is no separate construct for a partition. A `span` on
`network` shades the lanes it names, and the two sides of a split are not generally next to each
other, so naming lanes is the only thing that works.

## The derivation, entire

Every subject has a liveness timeline, built from its own `kills` and `revives` events. A message
is drawn by three readings of it — its sender when it departs, its receiver when it arrives, and
the run at that same moment. Whichever is not taking part names the picture: **never left**, **died
at the lifeline**, **still in flight**. A message written `-x` never arrives. Anything untouched
**arrived**.

Nothing else derives anything. A pause, a partition and a timer are pictures: the document already
carries the arrival that actually happened.

## Three outputs, one drawing

`--format svg | png | html`. PNG is the SVG rasterised; HTML is the SVG wrapped, with a hover panel
over the detail blocks. Neither is a second drawing, so a screenshot of the page and the exported
file are the same picture. Text is measured with an embedded face, so the same document renders to
the same bytes on every machine — which is what the golden tests rest on.

PNG needs `--features png`.

## Not in v1

Filtering (`--only`, `--between`), folding lanes together, and diffing two runs. Without filtering
this is comfortable at four nodes and gets crowded beyond; the compressed and ordinal axes help
with dense *time*, and nothing here helps with dense *lanes*.
