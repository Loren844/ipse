// The notes of a new version (issue #156), as the update window shows them. They come
// from the changelog, as Markdown or as the plain text the release script now makes:
// headings, items and paragraphs are told apart, and the marks (`**`, `*`, backticks,
// links) are taken out. Nothing is read as HTML.

export type NotesBlock = { kind: 'heading' | 'paragraph'; text: string } | { kind: 'list'; items: string[] };

function inline(text: string): string {
	return text
		.replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
		.replace(/\*\*(.+?)\*\*/g, '$1')
		.replace(/(^|[^\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])/g, '$1$2')
		.replace(/`/g, '')
		.trim();
}

export function notesBlocks(notes: string): NotesBlock[] {
	const blocks: NotesBlock[] = [];
	let paragraph: string[] = [];
	const flush = () => {
		if (paragraph.length) blocks.push({ kind: 'paragraph', text: inline(paragraph.join(' ')) });
		paragraph = [];
	};
	for (const raw of notes.split('\n')) {
		const line = raw.trim();
		const heading = /^#{1,6}\s+(.*)$/.exec(line);
		const item = /^(?:[-*•])\s+(.*)$/.exec(line);
		if (!line) {
			flush();
		} else if (heading) {
			flush();
			blocks.push({ kind: 'heading', text: inline(heading[1]) });
		} else if (item) {
			flush();
			const last = blocks.at(-1);
			if (last?.kind === 'list') last.items.push(inline(item[1]));
			else blocks.push({ kind: 'list', items: [inline(item[1])] });
		} else if (blocks.at(-1)?.kind === 'list' && raw.startsWith(' ') && !paragraph.length) {
			// An item that goes on over the next line.
			const list = blocks.at(-1) as { kind: 'list'; items: string[] };
			list.items[list.items.length - 1] += ' ' + inline(line);
		} else {
			paragraph.push(line);
		}
	}
	flush();
	return blocks;
}
