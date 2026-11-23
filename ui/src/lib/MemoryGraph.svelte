<script lang="ts" module>
	/** How each kind of file is drawn: a shape of its own, so that kinds are told apart
	 * without a colour code (docs/design/charter.md, section 4). */
	export const SHAPES: Record<string, 'circle' | 'square' | 'diamond' | 'triangle' | 'hexagon' | 'down' | 'cross' | 'dot'> = {
		person: 'circle',
		place: 'square',
		organization: 'diamond',
		project: 'triangle',
		work: 'hexagon',
		thing: 'down',
		topic: 'cross',
		event: 'dot'
	};
</script>

<script lang="ts">
	// The whole memory drawn on a canvas (issue #174): one drawing for tens of thousands
	// of files, moved and zoomed freely. The page around it holds what the keyboard and
	// screen readers need: the chosen file and its links, as a list.
	import { onMount } from 'svelte';
	import type { GraphView } from './api.ts';
	import { Grid, SPACING, bounds, type Laid } from './memoryGraph.ts';
	import { i18n } from './i18n.svelte.ts';

	let {
		graph,
		laid,
		shown,
		chosen = -1,
		onchoose
	}: {
		graph: GraphView;
		/** Where its files sit: `layoutMemory(graph)`. */
		laid: Laid;
		/** Whether the filters keep a file. */
		shown: (i: number) => boolean;
		chosen?: number;
		onchoose: (i: number) => void;
	} = $props();

	let canvas: HTMLCanvasElement;
	let ctx: CanvasRenderingContext2D | null = null;
	let width = 0;
	let height = 0;
	let dpr = 1;
	// The view: a point of the graph (x, y) is drawn at (x·k + tx, y·k + ty).
	let k = 1;
	let tx = 0;
	let ty = 0;
	let hovered = -1;
	let dirty = false;
	const frames: number[] = [];
	/** The files a region shows when the view is far out. */
	const HEART_FILES = 40;

	const grid = $derived(new Grid(laid));
	/** The files by importance: the heart of each region first. */
	const byImportance = $derived(Array.from(laid.rank.keys()).sort((a, b) => laid.rank[a]! - laid.rank[b]! || a - b));

	// What the filters keep, read once per change rather than once per frame.
	let visible = new Uint8Array(0);
	let edgePath: Path2D | null = null;
	let nodePaths = new Map<string, { filled: Path2D; hollow: Path2D }>();
	let pathRadius = 0;
	let counts: number[] = [];

	const colours = { ground: '#fff', node: '#6e6466', line: '#e5dedc', region: '#eee9e7', text: '#231d1e', muted: '#6e6466', thread: '#9b2c3a', surface: '#fff' };
	function readColours() {
		const style = getComputedStyle(document.documentElement);
		const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
		colours.ground = read('--ground', colours.ground);
		colours.surface = read('--surface', colours.surface);
		colours.node = read('--muted', colours.node);
		colours.line = read('--line-strong', colours.line);
		colours.region = read('--line', colours.region);
		colours.text = read('--text', colours.text);
		colours.muted = read('--muted', colours.muted);
		colours.thread = read('--thread', colours.thread);
		schedule();
	}

	const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
	/** The glide when the view moves to a file: `--motion-calm`. */
	function glideTime() {
		if (reduced) return 0;
		return parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--motion-calm')) || 220;
	}

	function refilter() {
		const n = graph.nodes.length;
		visible = new Uint8Array(n);
		for (let i = 0; i < n; i++) visible[i] = i === 0 && graph.centered ? 1 : shown(i) ? 1 : 0;
		counts = laid.regions.map((r) => r.files.reduce((sum, f) => sum + visible[f]!, 0));
		const edges = new Path2D();
		for (let e = 0; e + 1 < graph.edges.length; e += 2) {
			const a = graph.edges[e]!;
			const b = graph.edges[e + 1]!;
			if (!visible[a] || !visible[b]) continue;
			edges.moveTo(laid.x[a]!, laid.y[a]!);
			edges.lineTo(laid.x[b]!, laid.y[b]!);
		}
		edgePath = edges;
		pathRadius = 0;
		schedule();
	}

	/** One file's shape, centred on (x, y). */
	function shape(path: Path2D, kind: string, x: number, y: number, r: number) {
		switch (SHAPES[kind] ?? 'circle') {
			case 'square':
				path.rect(x - r * 0.85, y - r * 0.85, r * 1.7, r * 1.7);
				break;
			case 'diamond':
				path.moveTo(x, y - r * 1.15);
				path.lineTo(x + r * 1.15, y);
				path.lineTo(x, y + r * 1.15);
				path.lineTo(x - r * 1.15, y);
				path.closePath();
				break;
			case 'triangle':
				path.moveTo(x, y - r * 1.2);
				path.lineTo(x + r * 1.1, y + r * 0.8);
				path.lineTo(x - r * 1.1, y + r * 0.8);
				path.closePath();
				break;
			case 'down':
				path.moveTo(x, y + r * 1.2);
				path.lineTo(x + r * 1.1, y - r * 0.8);
				path.lineTo(x - r * 1.1, y - r * 0.8);
				path.closePath();
				break;
			case 'hexagon':
				for (let s = 0; s < 6; s++) {
					const a = (Math.PI / 3) * s;
					if (s === 0) path.moveTo(x + r * Math.cos(a), y + r * Math.sin(a));
					else path.lineTo(x + r * Math.cos(a), y + r * Math.sin(a));
				}
				path.closePath();
				break;
			case 'cross': {
				const w = r * 0.45;
				path.rect(x - r, y - w, 2 * r, 2 * w);
				path.rect(x - w, y - r, 2 * w, 2 * r);
				break;
			}
			case 'dot':
				path.moveTo(x + r * 0.7, y);
				path.arc(x, y, r * 0.7, 0, 2 * Math.PI);
				break;
			default:
				path.moveTo(x + r, y);
				path.arc(x, y, r, 0, 2 * Math.PI);
		}
	}

	/** The files' shapes, at a size that stays visible however far the view is out. */
	function buildNodes(radius: number) {
		const paths = new Map<string, { filled: Path2D; hollow: Path2D }>();
		const owner = graph.centered ? 0 : -1;
		graph.nodes.forEach((node, i) => {
			if (!visible[i] || i === owner) return;
			const kind = node.kind;
			let entry = paths.get(kind);
			if (!entry) {
				entry = { filled: new Path2D(), hollow: new Path2D() };
				paths.set(kind, entry);
			}
			shape(node.sensitive ? entry.hollow : entry.filled, kind, laid.x[i]!, laid.y[i]!, radius);
		});
		nodePaths = paths;
		pathRadius = radius;
	}

	function schedule() {
		if (dirty || !ctx) return;
		dirty = true;
		requestAnimationFrame(draw);
	}

	const toScreen = (i: number) => ({ x: laid.x[i]! * k + tx, y: laid.y[i]! * k + ty });
	const label = (i: number) => {
		const node = graph.nodes[i]!;
		const name = i === 0 && graph.centered ? i18n.t.graphYou : node.name;
		return name.length > 28 ? `${name.slice(0, 27)}…` : name;
	};

	function draw() {
		dirty = false;
		if (!ctx) return;
		const began = performance.now();
		const c = ctx;
		c.setTransform(dpr, 0, 0, dpr, 0, 0);
		c.fillStyle = colours.surface;
		c.fillRect(0, 0, width, height);

		// Regions.
		c.setTransform(dpr * k, 0, 0, dpr * k, dpr * tx, dpr * ty);
		c.fillStyle = colours.region;
		laid.regions.forEach((r, n) => {
			if (!counts[n]) return;
			c.beginPath();
			c.arc(r.x, r.y, r.radius + SPACING, 0, 2 * Math.PI);
			c.fill();
		});

		// Far out, files stand closer than a few pixels: each region shows only its
		// heart, its most linked files, and its count; zooming in shows every file.
		const crowded = SPACING * k < 3;

		// Links, faint.
		if (edgePath && !crowded) {
			c.strokeStyle = colours.line;
			c.lineWidth = 1 / k;
			c.globalAlpha = 0.7;
			c.stroke(edgePath);
			c.globalAlpha = 1;
		}

		c.fillStyle = colours.node;
		c.strokeStyle = colours.node;
		c.lineWidth = 1.5 / k;
		if (crowded) {
			const heart = new Path2D();
			const hollow = new Path2D();
			laid.regions.forEach((region) => {
				let drawn = 0;
				for (const f of region.files) {
					if (drawn >= HEART_FILES) break;
					if (!visible[f]) continue;
					drawn++;
					shape(graph.nodes[f]!.sensitive ? hollow : heart, graph.nodes[f]!.kind, laid.x[f]!, laid.y[f]!, 3 / k);
				}
			});
			c.fill(heart);
			c.stroke(hollow);
			pathRadius = 3 / k;
		} else {
			// Files at their real size, never under 1.6 px.
			const wanted = Math.max(4, 1.6 / k);
			const bucket = 2 ** Math.ceil(Math.log2(wanted));
			if (bucket !== pathRadius) buildNodes(bucket);
			for (const { filled, hollow } of nodePaths.values()) {
				c.fill(filled);
				c.stroke(hollow);
			}
		}

		// The chosen file and the one under the pointer: their links, in the thread's red.
		const lit = [chosen, hovered].filter((i, at, all) => i >= 0 && visible[i] && all.indexOf(i) === at);
		const neighbours = new Set<number>();
		c.strokeStyle = colours.thread;
		c.lineWidth = 1.6 / k;
		for (const i of lit) {
			c.beginPath();
			for (let at = laid.start[i]!; at < laid.start[i + 1]!; at++) {
				const j = laid.links[at]!;
				if (!visible[j]) continue;
				neighbours.add(j);
				c.moveTo(laid.x[i]!, laid.y[i]!);
				c.lineTo(laid.x[j]!, laid.y[j]!);
			}
			c.stroke();
		}
		const r = pathRadius;
		c.fillStyle = colours.thread;
		for (const j of neighbours) {
			const p = new Path2D();
			shape(p, graph.nodes[j]!.kind, laid.x[j]!, laid.y[j]!, r);
			c.fill(p);
		}
		if (graph.centered) {
			c.beginPath();
			c.arc(0, 0, Math.max(9, 7 / k), 0, 2 * Math.PI);
			c.fill();
		}
		for (const i of lit) {
			c.beginPath();
			c.arc(laid.x[i]!, laid.y[i]!, r * 2, 0, 2 * Math.PI);
			c.lineWidth = 2.5 / k;
			c.stroke();
		}

		// Labels, on the screen's grid: the regions' names and counts, then as many files'
		// names as fit, the heart of each region first.
		c.setTransform(dpr, 0, 0, dpr, 0, 0);
		c.textBaseline = 'middle';
		const taken: [number, number, number, number][] = [];
		const free = (x0: number, y0: number, x1: number, y1: number) =>
			taken.every(([a, b, cc, d]) => x1 < a || x0 > cc || y1 < b || y0 > d);
		c.font = `700 14px ${getComputedStyle(canvas).fontFamily}`;
		c.textAlign = 'center';
		laid.regions.forEach((region, n) => {
			if (!counts[n]) return;
			const x = region.x * k + tx;
			const y = (region.y - region.radius - SPACING) * k + ty - 12;
			const text = `${i18n.t.kinds[region.kind] ?? region.kind} · ${counts[n]}`;
			const w = text.length * 7.5;
			c.fillStyle = colours.text;
			c.fillText(text, x, y);
			taken.push([x - w / 2, y - 9, x + w / 2, y + 9]);
		});
		c.font = `13px ${getComputedStyle(canvas).fontFamily}`;
		c.textAlign = 'left';
		const must = [...lit, ...neighbours];
		let placed = 0;
		const tryLabel = (i: number, strong: boolean) => {
			const p = toScreen(i);
			if (p.x < -50 || p.y < -20 || p.x > width + 50 || p.y > height + 20) return;
			const text = label(i);
			const gap = (i === 0 && graph.centered ? Math.max(9 * k, 7) : Math.max(r * k, 3.5)) + 4;
			const w = text.length * 6.6;
			if (!free(p.x + gap - 2, p.y - 9, p.x + gap + w + 2, p.y + 9) && !strong) return;
			taken.push([p.x + gap - 2, p.y - 9, p.x + gap + w + 2, p.y + 9]);
			c.fillStyle = strong ? colours.text : colours.muted;
			c.lineWidth = 3;
			c.strokeStyle = colours.surface;
			c.strokeText(text, p.x + gap, p.y);
			c.fillText(text, p.x + gap, p.y);
			placed++;
		};
		for (const i of must.slice(0, 80)) tryLabel(i, true);
		for (const i of byImportance) {
			if (placed >= 220) break;
			if (!visible[i] || must.includes(i)) continue;
			// Only where files stand far enough apart for a name.
			if (laid.rank[i]! > 3 && SPACING * k < 9) continue;
			if (crowded && laid.rank[i]! >= HEART_FILES) continue;
			tryLabel(i, i === 0 && graph.centered);
		}

		const took = performance.now() - began;
		frames.push(took);
		if (frames.length > 120) frames.shift();
		const sorted = [...frames].sort((a, b) => a - b);
		canvas.dataset.frame = sorted[Math.floor(sorted.length / 2)]!.toFixed(2);
		canvas.dataset.worst = sorted[sorted.length - 1]!.toFixed(2);
		canvas.dataset.frames = String(frames.length);
	}

	function fit() {
		const [x0, y0, x1, y1] = bounds(laid);
		const margin = 40;
		k = Math.min((width - margin * 2) / (x1 - x0), (height - margin * 2) / (y1 - y0), 3);
		tx = width / 2 - ((x0 + x1) / 2) * k;
		ty = height / 2 - ((y0 + y1) / 2) * k;
		schedule();
	}

	let glide = 0;
	/** Moves the view to `i`, zoomed in enough to read the names around it. */
	export function centerOn(i: number) {
		if (i < 0 || i >= graph.nodes.length || !width) return;
		const to = { k: Math.max(k, 1.4) };
		const goal = { k: to.k, tx: width / 2 - laid.x[i]! * to.k, ty: height / 2 - laid.y[i]! * to.k };
		const from = { k, tx, ty };
		const time = glideTime();
		cancelAnimationFrame(glide);
		if (!time) {
			({ k, tx, ty } = goal);
			schedule();
			return;
		}
		const began = performance.now();
		const step = (now: number) => {
			const t = Math.min(1, (now - began) / time);
			const e = 1 - (1 - t) ** 3;
			k = from.k + (goal.k - from.k) * e;
			tx = from.tx + (goal.tx - from.tx) * e;
			ty = from.ty + (goal.ty - from.ty) * e;
			schedule();
			if (t < 1) glide = requestAnimationFrame(step);
		};
		glide = requestAnimationFrame(step);
	}

	function zoom(factor: number, x = width / 2, y = height / 2) {
		const next = Math.min(8, Math.max(0.02, k * factor));
		tx = x - ((x - tx) * next) / k;
		ty = y - ((y - ty) * next) / k;
		k = next;
		schedule();
	}

	function pan(dx: number, dy: number) {
		tx += dx;
		ty += dy;
		schedule();
	}

	const pointers = new Map<number, { x: number; y: number }>();
	let moved = 0;
	let pinch = 0;

	function local(event: PointerEvent | WheelEvent) {
		const box = canvas.getBoundingClientRect();
		return { x: event.clientX - box.left, y: event.clientY - box.top };
	}

	function hit(x: number, y: number) {
		return grid.nearest(laid, (x - tx) / k, (y - ty) / k, Math.max(10 / k, pathRadius * 1.5), (i) => visible[i] === 1 && (SPACING * k >= 3 || laid.rank[i]! < HEART_FILES));
	}

	function down(event: PointerEvent) {
		canvas.setPointerCapture(event.pointerId);
		pointers.set(event.pointerId, local(event));
		moved = 0;
		cancelAnimationFrame(glide);
		if (pointers.size === 2) {
			const [a, b] = [...pointers.values()];
			pinch = Math.hypot(a!.x - b!.x, a!.y - b!.y);
		}
	}

	function move(event: PointerEvent) {
		const p = local(event);
		const before = pointers.get(event.pointerId);
		if (!before) {
			const found = hit(p.x, p.y);
			if (found !== hovered) {
				hovered = found;
				canvas.style.cursor = found >= 0 ? 'pointer' : 'grab';
				schedule();
			}
			return;
		}
		pointers.set(event.pointerId, p);
		if (pointers.size === 2) {
			const [a, b] = [...pointers.values()];
			const distance = Math.hypot(a!.x - b!.x, a!.y - b!.y);
			if (pinch) zoom(distance / pinch, (a!.x + b!.x) / 2, (a!.y + b!.y) / 2);
			pinch = distance;
			moved += 10;
			return;
		}
		moved += Math.abs(p.x - before.x) + Math.abs(p.y - before.y);
		pan(p.x - before.x, p.y - before.y);
	}

	function up(event: PointerEvent) {
		const p = local(event);
		const was = pointers.size;
		pointers.delete(event.pointerId);
		if (pointers.size < 2) pinch = 0;
		if (was === 1 && moved < 5) {
			const found = hit(p.x, p.y);
			if (found >= 0) onchoose(found);
		}
	}

	function wheel(event: WheelEvent) {
		event.preventDefault();
		const p = local(event);
		const delta = event.deltaMode === 1 ? event.deltaY * 16 : event.deltaY;
		zoom(Math.exp(-delta * 0.0015), p.x, p.y);
	}

	function key(event: KeyboardEvent) {
		const step = 80;
		const actions: Record<string, () => void> = {
			ArrowLeft: () => pan(step, 0),
			ArrowRight: () => pan(-step, 0),
			ArrowUp: () => pan(0, step),
			ArrowDown: () => pan(0, -step),
			'+': () => zoom(1.25),
			'=': () => zoom(1.25),
			'-': () => zoom(0.8),
			'0': fit
		};
		const action = actions[event.key];
		if (action) {
			event.preventDefault();
			action();
		}
	}

	$effect(() => {
		void laid;
		void shown;
		if (ctx) refilter();
	});
	$effect(() => {
		void chosen;
		schedule();
	});

	let fitted = false;
	onMount(() => {
		ctx = canvas.getContext('2d');
		readColours();
		refilter();
		const resize = () => {
			const box = canvas.parentElement!.getBoundingClientRect();
			const was = { width, height };
			width = Math.max(1, box.width);
			height = Math.max(1, box.height);
			dpr = window.devicePixelRatio || 1;
			canvas.width = Math.round(width * dpr);
			canvas.height = Math.round(height * dpr);
			canvas.style.width = `${width}px`;
			canvas.style.height = `${height}px`;
			if (!fitted) {
				fitted = true;
				fit();
				if (chosen >= 0 && !(chosen === 0 && graph.centered)) centerOn(chosen);
			} else {
				tx += (width - was.width) / 2;
				ty += (height - was.height) / 2;
				schedule();
			}
		};
		const observer = new ResizeObserver(resize);
		observer.observe(canvas.parentElement!);
		resize();
		const theme = new MutationObserver(readColours);
		theme.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
		canvas.addEventListener('wheel', wheel, { passive: false });
		return () => {
			observer.disconnect();
			theme.disconnect();
			canvas.removeEventListener('wheel', wheel);
			cancelAnimationFrame(glide);
			ctx = null;
		};
	});

	const files = $derived(graph.nodes.length);
	const links = $derived(graph.edges.length / 2);
</script>

<!-- A map-like drawing, as on the map (issue #129): the keys move and zoom it. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
	class="memory-graph-canvas"
	tabindex="0"
	role="application"
	aria-label={i18n.t.graphLabel(files, links)}
	aria-describedby="graph-keys"
	onkeydown={key}
>
	<canvas
		bind:this={canvas}
		data-testid="memory-graph"
		data-files={files}
		aria-hidden="true"
		onpointerdown={down}
		onpointermove={move}
		onpointerup={up}
		onpointercancel={up}
		onpointerleave={() => {
			if (hovered >= 0 && !pointers.size) {
				hovered = -1;
				schedule();
			}
		}}
	></canvas>
	<div class="graph-zoom">
		<button type="button" class="icon-button" aria-label={i18n.t.zoomIn} onclick={() => zoom(1.4)}>+</button>
		<button type="button" class="icon-button" aria-label={i18n.t.zoomOut} onclick={() => zoom(1 / 1.4)}>−</button>
		<button type="button" class="icon-button" aria-label={i18n.t.zoomAll} onclick={fit}>
			<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" /></svg>
		</button>
	</div>
	<p id="graph-keys" class="visually-hidden">{i18n.t.graphKeys}</p>
</div>
