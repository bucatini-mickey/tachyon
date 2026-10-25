/**
 * pterm.ts - Tachyon's web pseudo-terminal.
 *
 * One monospace <pre> of cells. A cell is a glyph with a foreground and a
 * background, each an index into the palette: 16 inks, then SHADES darker
 * steps of each ink (the same table as the Rust engine's `draw::palette`), so
 * light can fall off smoothly without leaving the ink's hue. Nothing else is
 * drawn: no images, no HTML
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

/** Darker steps per ink: how much of the ink each keeps, darkest first. Mirrors `palette::SHADES`. */
export const SHADES = [0.22, 0.4, 0.58, 0.78];

/** The index of an ink's shade step: step 0 is the darkest; SHADES.length or more is the ink itself. */
export const shade = (ink: number, step: number): number => (step >= SHADES.length ? ink : 16 + ink * SHADES.length + Math.max(0, step));

/** The 16 inks followed by their shade steps (a palette already expanded comes back as it is). */
export function expand(p: Palette): Palette {
	if (p.length !== 16) return p;
	const out = [...p];
	for (const hex of p) {
		const rgb = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
		for (const k of SHADES) out.push('#' + rgb.map((v) => Math.round(v * k).toString(16).padStart(2, '0')).join(''));
	}
	return out;
}

export interface Cell {
	ch: string;
	fg: number;
	bg: number;
	/** The glyph is centred across this cell and the next (whose own glyph is not drawn). */
	wide?: boolean;
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
		for (const c of this.cells) { c.ch = ' '; c.fg = fg; c.bg = bg; c.wide = false; }
		this.hits = [];
	}

	set(x: number, y: number, ch: string, fg: number, bg?: number) {
		if (x < 0 || y < 0 || x >= this.cols || y >= this.rows) return;
		const i = y * this.cols + x;
		const c = this.cells[i];
		// Writing into either half of a wide glyph splits it back into two ordinary cells.
		if (c.wide && x + 1 < this.cols) { c.wide = false; this.cells[i + 1].ch = ' '; }
		if (x > 0 && this.cells[i - 1].wide) { this.cells[i - 1].wide = false; }
		c.ch = ch;
		c.fg = fg;
		if (bg !== undefined) c.bg = bg;
	}

	/** A glyph centred across two cells (x and x+1), so a square tile of two columns holds it in its middle. */
	setWide(x: number, y: number, ch: string, fg: number, bg?: number) {
		if (x < 0 || y < 0 || x + 1 >= this.cols || y >= this.rows) { this.set(x, y, ch, fg, bg); return; }
		this.set(x, y, ch, fg, bg);
		this.set(x + 1, y, '', fg, bg);
		this.cells[y * this.cols + x].wide = true;
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

	/** A box with an optional title in the top rule: plain ASCII, or single or double CP437 lines. */
	box(r: Rect, fg: number, bg: number, title = '', titleFg = fg, style: 'ascii' | 'single' | 'double' = 'ascii') {
		const { x, y, w, h } = r;
		if (w < 2 || h < 2) return;
		const [tl, tr, bl, br, hz, vt] = style === 'double' ? ['╔', '╗', '╚', '╝', '═', '║'] : style === 'single' ? ['┌', '┐', '└', '┘', '─', '│'] : ['+', '+', '+', '+', '-', '|'];
		this.set(x, y, tl, fg, bg);
		this.set(x + w - 1, y, tr, fg, bg);
		this.set(x, y + h - 1, bl, fg, bg);
		this.set(x + w - 1, y + h - 1, br, fg, bg);
		for (let i = 1; i < w - 1; i++) { this.set(x + i, y, hz, fg, bg); this.set(x + i, y + h - 1, hz, fg, bg); }
		for (let j = 1; j < h - 1; j++) { this.set(x, y + j, vt, fg, bg); this.set(x + w - 1, y + j, vt, fg, bg); }
		if (title && w > 4) this.text(x + 2, y, ` ${title} `.slice(0, w - 4), titleFg, bg);
	}

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
	/** The grid is at least this many rows tall (default 24); on a wide, short screen the font shrinks to fit them. */
	rows?: number;
	/** Called when the app should redraw (size change, entry typed, `draw()`). */
	onDraw: (t: Term) => void;
	/** Desktop keys (PageUp, Escape...) not typed into the entry field. */
	onKey?: (key: string, ev: KeyboardEvent) => void;
}

const esc = (c: string) => (c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : c);

export class Term extends Grid {
	private raw: Palette | null = null;
	private pal: Palette = [];
	/** Each row's last written HTML, so a flush only touches the rows that changed. */
	private rowHtml: string[] = [];
	private rowEls: HTMLElement[] = [];
	/** The last frame: ms in the app's drawing, ms writing the DOM, and rows rewritten. */
	stats = { draw: 0, flush: 0, rows: 0 };
	/** The palette in use. Set 16 inks; their shade steps are added. */
	get palette(): Palette { return this.pal; }
	set palette(p: Palette) {
		if (p === this.raw) return;
		this.raw = p;
		this.pal = expand(p);
		this.rowHtml = [];
	}
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
	/** Where the last tap fell, and the field it opened for typing. */
	private tapAt: { x: number; y: number } | null = null;
	private entryAt: Rect | null = null;

	constructor(host: HTMLElement, opts: TermOptions) {
		super();
		this.opts = opts;
		this.palette = opts.palette ?? PALETTE;
		const doc = host.ownerDocument;
		host.style.cssText = 'position:fixed;inset:0;overflow:hidden;margin:0;padding:0;display:flex;justify-content:center;align-items:flex-start;touch-action:manipulation;';
		doc.documentElement.style.overflow = 'hidden';
		// Phones enlarge text they think is too small (font boosting); in a grid that breaks every line.
		doc.documentElement.style.setProperty('-webkit-text-size-adjust', '100%');
		doc.documentElement.style.setProperty('text-size-adjust', '100%');
		doc.body.style.cssText = 'margin:0;overflow:hidden;overscroll-behavior:none;';
		// Rows are blocks and runs fill them, so a ground colour covers the whole line with no seams.
		const css = doc.createElement('style');
		// A glyph outside ASCII sits in a box exactly one cell wide (two for a wide one), centred, so a
		// fallback font of another width can never push the rest of the line along.
		css.textContent = '.pt-row{display:block}.pt-row>span{display:inline-block;height:100%;vertical-align:top}'
			+ '.pt-row i{display:inline-block;width:var(--pt-cw);height:100%;text-align:center;font-style:normal;overflow:hidden;vertical-align:top}'
			+ '.pt-row i.w{width:calc(2 * var(--pt-cw))}';
		doc.head.appendChild(css);
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
			const x = Math.floor((ev.clientX - b.left) / this.cw), y = Math.floor((ev.clientY - b.top) / this.lh);
			const fn = this.hitAt(x, y);
			ev.preventDefault();
			// A tap on the field being typed in keeps typing; a tap anywhere else ends it (keeping what was
			// typed) and lets the keyboard go. Left open, every later tap would bring the keyboard back up.
			if (this.editing) {
				const r = this.entryAt;
				if (r && x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h) { this.input.focus(); return; }
				this.endEntry(true);
			} else if (doc.activeElement === this.input) this.input.blur();
			this.tapAt = { x, y };
			if (fn) fn();
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
		const t = this.tapAt;
		this.entryAt = t ? this.hits.slice().reverse().find(({ r }) => t.x >= r.x && t.x < r.x + r.w && t.y >= r.y && t.y < r.y + r.h)?.r ?? null : null;
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

	/** Ask for at least this many columns (a screen that wants a margin round its page); refits when it changes. */
	setCols(cols: number) {
		if ((this.opts.cols ?? 44) === cols) return;
		this.opts.cols = cols;
		this.fit();
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
		// On a wide, short screen the height matters too: keep at least `rows` rows.
		font = Math.max(8, Math.min(font, Math.floor(H / ((this.opts.rows ?? 24) * 1.2))));
		this.cw = font * ratio;
		this.lh = Math.round(font * 1.2);
		const cols = Math.max(20, Math.floor(W / this.cw));
		const rows = Math.max(10, Math.floor(H / this.lh));
		this.pre.style.fontSize = `${font}px`;
		this.pre.style.lineHeight = `${this.lh}px`;
		this.rowEls = [];
		this.pre.style.width = `${cols * this.cw}px`;
		this.pre.style.setProperty('--pt-cw', `${this.cw}px`);
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
			const t0 = performance.now();
			this.opts.onDraw(this);
			const t1 = performance.now();
			this.flush();
			this.stats.draw = t1 - t0;
			this.stats.flush = performance.now() - t1;
		});
	}

	/** Write the cells to the <pre> as runs of one colour pair, rewriting only the rows that changed. */
	flush() {
		const p = this.palette;
		if (this.rowEls.length !== this.rows) {
			const doc = this.pre.ownerDocument;
			this.pre.textContent = '';
			this.rowEls = [];
			this.rowHtml = [];
			for (let y = 0; y < this.rows; y++) {
				const el = doc.createElement('span');
				el.className = 'pt-row';
				el.style.height = `${this.lh}px`;
				this.rowEls.push(el);
				this.pre.append(el);
			}
		}
		let n = 0;
		const out: string[] = [];
		for (let y = 0; y < this.rows; y++) {
			out.length = 0;
			let cur = -1;
			for (let x = 0; x < this.cols; x++) {
				const c = this.cells[y * this.cols + x];
				const key = c.fg * 256 + c.bg;
				if (key !== cur) {
					if (cur >= 0) out.push('</span>');
					out.push(`<span style="color:${p[c.fg] ?? p[4]};background:${p[c.bg] ?? p[0]}">`);
					cur = key;
				}
				if (c.wide) { out.push(`<i class="w">${esc(c.ch)}</i>`); x++; }
				// Half blocks are painted as two exact halves of colour, so pixel pictures have no seams.
				else if (c.ch === '▀' || c.ch === '▄') {
					const [hi, lo] = c.ch === '▀' ? [p[c.fg] ?? p[4], p[c.bg] ?? p[0]] : [p[c.bg] ?? p[0], p[c.fg] ?? p[4]];
					out.push(`<i style="background:linear-gradient(${hi} 50%,${lo} 50%)"> </i>`);
				}
				else if (c.ch.charCodeAt(0) > 126) out.push(`<i>${c.ch}</i>`);
				else out.push(esc(c.ch));
			}
			out.push('</span>');
			const html = out.join('');
			if (html !== this.rowHtml[y]) {
				this.rowHtml[y] = html;
				this.rowEls[y].innerHTML = html;
				n++;
			}
		}
		this.stats.rows = n;
	}
}
