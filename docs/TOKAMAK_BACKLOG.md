# Tokamak usability backlog (tach-004)

Read-only review of `examples/tokamak/main.rs` (1889 lines), `tokamak.bat`, `tokamak/` and CLAUDE.md. No code changed.
`cargo test --release --example tokamak`: all 10 tests pass (builds in about 9 s). I did not run the UI itself, so items marked (unverified) come from reading the code only.

What it is today: a terminal workshop with three tabs. Part (paint a blueprint cell by cell over several levels, with sockets), Library (materials and kinds as text) and Art (glyph pictures). It has undo/redo, a project picker, an unsaved-work prompt and mouse support. The basics are good. The list below is what stops it being a world editor, most important first.

## A. Blockers (it cannot do the job without these)

1. **Nothing reads its files.** Only `examples/tokamak/main.rs` knows `.tlib`, `.tpart` and `.tart`; no engine or Broadside code loads them (grep of `src/`, `examples/`, `tests/`, `../Broadside/src`). Parts are valid blueprints, but the library (glyphs, colours, tags) and the sockets never reach a game. Needed: a `tachyon::make::library` module (parse/write, moved out of the example) plus a socket-aware `Part` loader, so a game can stamp a part and snap sockets together. Rust note: this means moving `parse_library`, `Part` and `Art` into `src/make/`, making them `pub`, and having the example `use` them.
2. **No way to build a world, only pieces.** There is no map/atlas/region mode: no placing parts into a larger scene, no snapping on sockets, no preview of two parts joined. Sockets can be made but never checked (no "does this fit that" test, no list of the tags in use).
3. **Part size is fixed at creation.** Only the number of levels can change (`+`/`-`). No resize of width/height, no shift/move of the contents. Getting the size wrong means starting again.
4. **No select / copy / paste / move.** The only bulk tools are rectangle, room, flood fill and "copy the level below". Missing: select region, copy, paste, cut, move, rotate and flip a part, line tool, duplicate a whole part, copy between parts.
5. **Part names cannot be changed, and saving can silently overwrite.** The name is set only in Ctrl+N; there is no rename or Save As. A new part with an existing name overwrites that file on Ctrl+S with no warning.

## B. Crash and data-loss risks

6. **Undo memory on big parts (unverified).** Every checkpoint clones the whole part, up to 200 deep (`UNDO_DEPTH`). The size cap is 200 x 200 x 32 = 1.28 M cells, each holding Strings, so one snapshot could be tens of MB and 200 of them could run the PC out of memory. Fix: lower the depth for big parts, or store only the changed cells.
7. **Every Space press is its own undo step.** Keyboard painting calls `checkpoint()` per key, so 200 presses empties the history. Mouse strokes are fine (one per click or drag). Fix: merge consecutive keyboard paints.
8. **The `tokamak/` folder is untracked in git.** The Broadside project (library.tlib, empty art/ and parts/) and the `.last` file have no history or backup. Decide: commit projects, or add a backup/export step.
9. **Library text can be corrupted by typing.** Rename and tags accept `|`; the file format splits on " | ", so a name like `a | b` breaks the entry on reload. A glyph set to space is saved, then read back wrong (spaces are filtered when parsing, so the floor glyph slides into the wall slot). Fix: reject `|` and space in names, tags and glyphs.
10. **Names go straight into file paths.** Part, art and project names are not cleaned, so `..\x` writes outside the folder and `a/b` fails with "Could not save" (project names from the picker only swap spaces for `-`). Windows-reserved names (`con`, `nul`) will also misbehave.
11. **Corrupt files are only half-checked.** (DONE tach-011) `Part::from_text` ignores short socket lines, quietly turns an unknown direction into `n`, and accepts sockets outside the part's bounds. A blueprint with a negative size would try to allocate a huge Vec. Fix: validate on load and report line numbers.
12. **Renaming or deleting a library entry orphans parts.** Parts store material and thing names as text, so after a rename the part draws those cells in red (the "unknown" colour) and nothing warns or offers to update the parts.

## C. Unclear or broken keys

13. **Library Ctrl+1..6 (background nudge) never works.** The help says "hold Ctrl or Alt: the background", but the Ctrl branch in `on_key` returns before `library_key` sees the digit. Alt may work depending on the terminal. Fix: handle Ctrl+digit before that early return, or document Alt only.
14. **Ctrl+N / Ctrl+O act on the wrong thing in the Library tab.** There they still open "new part" and the parts picker (only Art mode switches to art). Should be disabled or switch tab, and say so in the footer.
15. **Esc quits the program.** Esc also cancels, closes and drops a corner, so one extra press on a clean file exits instantly (with unsaved work it asks). Suggest Ctrl+Q to quit and Esc only to cancel.
16. **Ctrl+S saves only the current tab.** Each tab shows `*` when unsaved, but only the quit prompt's `s` saves all. Add an explicit "save all" and show which tabs are unsaved in the footer.
17. **Key clashes and memory load.** `u`/`n` are stairs while Ctrl+N is new; `r`/`b` mean different things per tab; `k` is "things on/off" in Part but "new kind" in Library; `g` takes the next key as the glyph. Fine once learned, but the on-screen panel hints cover only a handful. A context line listing the real keys for the current state would help.
18. **Socket editing is thin.** One socket per cell, direction only cycles forward, the tag is retyped each time, sockets are not mirrored when mirror is on, and the panel lists only the first 5. Missing: list/jump to sockets, pick from known tags, edit a tag.
19. **No Ramp or StairUpDown brush.** `look()` draws both and the blueprint format supports them, but only five tiles are on the brush row, so they cannot be placed.
20. **Help is easy to lose.** The footer shows `? for every key` only when no note is set; after the first action it is gone. The top-bar hint is faint and gets clipped on narrow windows.

## D. Editing comfort

21. **Level view.** Only the level directly below is shown faintly. No view of the level above, no all-levels overview, no jump to a level by number.
22. **No way to see the part as a game will.** No test stamp inside a small world, no lighting or ASCII-scheme preview (Ctrl+T / Ctrl+G exist engine-wide; unverified here).
23. **Art tool limits.** Only the 16-colour palette; no resize of existing art, no line/ellipse, no select/copy/paste, no import of ASCII text, no export to plain text or HTML (Frame already has `to_text`/`to_html`). Canvas max 300 x 200.
24. **Library tools.** Colours nudge by 8 per key or are typed as numbers; no colour picker, no search, no sort, no copying entries between projects, and no link from a library material to the physics registry (`Materials::standard()`).
25. **Mouse.** No drag-select, no hover info, no pan, no click to place a socket. The wheel changes level in Part, scrolls in Library, and does nothing in Art.
26. **Small terminals.** The side panel is a fixed 34 columns and the canvas is clamped to at least 10, so below about 45 columns they overlap; the panel is cut off when there are many materials or kinds. The draw test covers only 120x36 and 50x14.

## E. Housekeeping

27. **Paths are relative to the working directory** (`"tokamak"`, `"tokamak/.last"`). `tokamak.bat` does `cd` first, but `cargo run --example tokamak` from another folder makes a second, empty `tokamak/` there.
28. **Saves are plain `fs::write`** (temp+rename DONE tach-011; no auto-save yet) (no temp file then rename), so a crash mid-save can truncate a file. No auto-save and no recovery file.
29. **Tests cover the logic, not input.** No test drives `on_key`/`on_mouse` (one would have caught item 13), and none opens a malformed file.
30. **Good as found:** the terminal is restored on panic, the loop redraws only on input, unsaved-work prompts work on quit/new/open/project switch, and all draw paths survive tiny windows.

## Suggested order of work
1. Item 1 (shared loader in `src/make/`) and items 9, 10, 11 (safe text and paths): small, and unlocks the rest.
2. Items 3, 4, 5 (resize, select/copy/paste, rename/save-as/overwrite guard).
3. Items 6, 7, 12, 13, 14, 15 (cheap fixes and bugs).
4. Item 2 (assembling parts into a scene) once a game consumes the files.
5. Section D when the basics are comfortable.
