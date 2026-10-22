/**
 * pterm.ts - Tachyon's web pseudo-terminal.
 *
 * One monospace <pre> of cells. A cell is a glyph with a foreground and a
 * background, each an index into a 16-ink palette (same idea as the Rust
 * engine's `draw::palette`). Nothing else is drawn: no images, no HTML
 * buttons, no proportional text. The grid is sized to fill the viewport
 * exactly, so the page never scrolls.
 *
 * Input: a tap becomes the cell under it and fires the topmost hit region
 * covering that cell; text entry goes through one hidden <input> that the app
 * echoes into cells itself (read `term.entry`).
 *
 * Grimoire vendors a copy of this file (src/term/pterm.ts); a test keeps
 * the two identical. Edit it here.
 */

export type Palette = readonly string[];

/** Same 16 inks as `src/draw/palette.rs`. */
export const PALETTE: Palette = [
	'#0b0d12', '#262c3a', '#46566e', '#7e8492', '#cecfd4', '#fff0be', '#e8c460', '#e68c3c',
	'#e86050', '#822822', '#8cc878', '#2e6038', '#6096e6', '#243874', '#a064be', '#6e4c2c'
];

export interface Cell {
	ch: string;
	fg: number;
	bg: number;
}

export interface Rect {
	x: number;
	y: number;
	w: number;
	h: number;
}

/** The cell buffer: all the drawing, with no DOM, so it can be tested anywhere. */
export class Grid {
	cols = 0;
	rows = 0;
	cells: Cell[] = [];
	/** Hit regions for this frame; later ones sit on top. */
	hits: { r: Rect; fn: (x: number, y: number) => void }[] = [];

	resize(cols: number, rows: number) {
		this.cols = cols;
		this.rows = rows;
		this.cells = Array.from({ length: cols * rows }, () => ({ ch: ' ', fg: 4, bg: 0 }));
		this.hits = [];
	}

	clear(fg = 4, bg = 0) {
		for (const c of this.cells) { c.ch = ' '; c.fg = fg; c.bg = bg; }
		this.hits = [];
	}

	set(x: number, y: number, ch: string, fg: number, bg?: number) {
		if (x < 0 || y < 0 || x >= this.cols || y >= this.rows) return;
		const c = this.cells[y * this.cols + x];
		c.ch = ch;
		c.fg = fg;
		if (bg !== undefined) c.bg = bg;
	}

	get(x: number, y: number): Cell | undefined {
		return x < 0 || y < 0 || x >= this.cols || y >= this.rows ? undefined : this.cells[y * this.cols + x];
	}

	/** Draw a string; returns the x after it. */
	text(x: number, y: number, s: string, fg: number, bg?: number): number {
		for (const ch of s) this.set(x++, y, ch, fg, bg);
		return x;
	}

	fill(r: Rect, ch: string, fg: number, bg: number) {
		for (let y = r.y; y < r.y + r.h; y++) for (let x = r.x; x < r.x + r.w; x++) this.set(x, y, ch, fg, bg);
	}

	/** Recolour a region without changing its glyphs. */
	tint(r: Rect, fg: number | null, bg: number | null) {
		for (let y = r.y; y < r.y + r.h; y++)
			for (let x = r.x; x < r.x + r.w; x++) {
				const c = this.get(x, y);
				if (!c) continue;
				if (fg !== null) c.fg = fg;
				if (bg !== null) c.bg = bg;
			}
	}

	/** An ASCII box: + - | corners, optional title in the top rule. */
	box(r: Rect, fg: number, bg: number, title = '', titleFg = fg) {
		const { x, y, w, h } = r;
		if (w < 2 || h < 2) return;
		this.set(x, y, '+', fg, bg);
		this.set(x + w - 1, y, '+', fg, bg);
		this.set(x, y + h - 1, '+', fg, bg);
		this.set(x + w - 1, y + h - 1, '+', fg, bg);
		for (let i = 1; i < w - 1; i++) { this.set(x + i, y, '-', fg, bg); this.set(x + i, y + h - 1, '-', fg, bg); }
		for (let j = 1; j < h - 1; j++) { this.set(x, y + j, '|', fg, bg); this.set(x + w - 1, y + j, '|', fg, bg); }
		if (title && w > 4) this.text(x + 2, y, ` ${title} `.slice(0, w - 4), titleFg, bg);
	}

	/** Register a tap region; `fn` gets the cell tapped. */
	hit(r: Rect, fn: (x: number, y: number) => void) {
		this.hits.push({ r, fn });
	}

	/** The action under a cell, if any (topmost first). */
	hitAt(x: number, y: number): (() => void) | null {
		for (let i = this.hits.length - 1; i >= 0; i--) {
			const { r, fn } = this.hits[i];
			if (x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h) return () => fn(x, y);
		}
		return null;
	}

	/** The grid as plain text (for tests and snapshots). */
	toText(): string {
		const out: string[] = [];
		for (let y = 0; y < this.rows; y++) out.push(this.cells.slice(y * this.cols, (y + 1) * this.cols).map((c) => c.ch).join('').trimEnd());
		return out.join('\n');
	}
}

export interface TermOptions {
	palette?: Palette;
	/** The grid is at least this many columns wide; the font is chosen to fit. */
	cols?: number;
	/** Largest font in px (so desktops do not get giant letters). */
	maxFont?: number;
	/** Called when the app should redraw (size change, entry typed, `draw()`). */
	onDraw: (t: Term) => void;
	/** Desktop keys (PageUp, Escape...) not typed into the entry field. */
	onKey?: (key: string, ev: KeyboardEvent) => void;
}

const esc = (c: string) => (c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : c);

export class Term extends Grid {
	palette: Palette;
	readonly pre: HTMLPreElement;
	private input: HTMLInputElement;
	private opts: TermOptions;
	private cw = 8;
	private lh = 16;
	private queued = false;
	/** Text being typed (empty when no entry is open). */
	entry = '';
	/** Whether the hidden input is taking text. */
	editing = false;
	private entryDone: ((v: string | null) => void) | null = null;

	constructor(host: HTMLElement, opts: TermOptions) {
		super();
		this.opts = opts;
		this.palette = opts.palette ?? PALETTE;
		const doc = host.ownerDocument;
		host.style.cssText = 'position:fixed;inset:0;overflow:hidden;margin:0;padding:0;display:flex;justify-content:center;align-items:flex-start;touch-action:manipulation;';
		doc.documentElement.style.overflow = 'hidden';
		doc.body.style.cssText = 'margin:0;overflow:hidden;overscroll-behavior:none;';
		this.pre = doc.createElement('pre');
		this.pre.style.cssText = 'margin:0;font-family:ui-monospace,"Cascadia Mono",Consolas,Menlo,"DejaVu Sans Mono","Liberation Mono",monospace;white-space:pre;user-select:none;-webkit-user-select:none;cursor:default;font-variant-ligatures:none;';
		host.appendChild(this.pre);
		this.input = doc.createElement('input');
		this.input.style.cssText = 'position:fixed;left:0;bottom:0;width:1px;height:1px;opacity:0;font-size:16px;border:0;padding:0;';
		this.input.autocomplete = 'off';
		this.input.setAttribute('autocapitalize', 'off');
		this.input.setAttribute('aria-label', 'text entry');
		host.appendChild(this.input);
		this.input.addEventListener('input', () => { this.entry = this.input.value; this.draw(); });
		this.input.addEventListener('keydown', (ev) => {
			if (ev.key === 'Enter') this.endEntry(true);
			else if (ev.key === 'Escape') this.endEntry(false);
		});
		this.pre.addEventListener('pointerdown', (ev) => {
			const b = this.pre.getBoundingClientRect();
			const fn = this.hitAt(Math.floor((ev.clientX - b.left) / this.cw), Math.floor((ev.clientY - b.top) / this.lh));
			ev.preventDefault();
			if (fn) fn();
			else if (this.editing) this.input.focus();
		});
		doc.addEventListener('keydown', (ev) => {
			if (ev.target === this.input || ev.ctrlKey || ev.metaKey || ev.altKey) return;
			this.opts.onKey?.(ev.key, ev);
		});
		doc.defaultView?.addEventListener('resize', () => this.fit());
		this.fit();
	}

	/** Open text entry (call from a tap so phones raise the keyboard). `done(null)` means cancelled. */
	startEntry(initial: string, maxLen: number, done: (v: string | null) => void) {
		this.entry = initial;
		this.input.value = initial;
		this.input.maxLength = maxLen;
		this.editing = true;
		this.entryDone = done;
		this.input.focus();
		this.draw();
	}

	endEntry(ok: boolean) {
		if (!this.editing) return;
		const done = this.entryDone;
		this.editing = false;
		this.entryDone = null;
		this.input.blur();
		done?.(ok ? this.entry : null);
		this.draw();
	}

	/** Measure a glyph, pick the font so `cols` columns fit the width, then count rows to fill the height. */
	fit() {
		const doc = this.pre.ownerDocument;
		const win = doc.defaultView!;
		const W = win.innerWidth, H = win.innerHeight;
		const want = this.opts.cols ?? 44;
		const probe = doc.createElement('span');
		probe.style.cssText = `position:absolute;visibility:hidden;font:100px ${this.pre.style.fontFamily};white-space:pre;`;
		probe.textContent = '0'.repeat(20);
		doc.body.appendChild(probe);
		const ratio = probe.getBoundingClientRect().width / 20 / 100;
		probe.remove();
		let font = Math.min(this.opts.maxFont ?? 24, Math.floor(W / (want * ratio)));
		// On a wide, short screen the height matters too: keep at least 24 rows.
		font = Math.max(8, Math.min(font, Math.floor(H / (24 * 1.2))));
		this.cw = font * ratio;
		this.lh = Math.round(font * 1.2);
		const cols = Math.max(20, Math.floor(W / this.cw));
		const rows = Math.max(10, Math.floor(H / this.lh));
		this.pre.style.fontSize = `${font}px`;
		this.pre.style.lineHeight = `${this.lh}px`;
		this.pre.style.width = `${cols * this.cw}px`;
		this.pre.style.height = `${rows * this.lh}px`;
		this.resize(cols, rows);
		this.draw();
	}

	/** Ask for a redraw on the next frame (coalesced). */
	draw() {
		if (this.queued) return;
		this.queued = true;
		const win = this.pre.ownerDocument.defaultView!;
		win.requestAnimationFrame(() => {
			this.queued = false;
			this.opts.onDraw(this);
			this.flush();
		});
	}

	/** Write the cells to the <pre> as runs of one colour pair. */
	flush() {
		const p = this.palette;
		const out: string[] = [];
		for (let y = 0; y < this.rows; y++) {
			let cur = -1;
			for (let x = 0; x < this.cols; x++) {
				const c = this.cells[y * this.cols + x];
				const key = c.fg * 16 + c.bg;
				if (key !== cur) {
					if (cur >= 0) out.push('</span>');
					out.push(`<span style="color:${p[c.fg]};background:${p[c.bg]}">`);
					cur = key;
				}
				out.push(esc(c.ch));
			}
			out.push('</span>\n');
		}
		this.pre.innerHTML = out.join('');
	}
}
