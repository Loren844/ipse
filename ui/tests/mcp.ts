// A minimal MCP client over stdio, as an AI app runs `ipse mcp`: enough to call tools
// in browser tests that start from what an AI really proposed.
import { spawn } from 'node:child_process';
import { IPSE } from './ipse.ts';

export type Ai = { call: (tool: string, args?: Record<string, unknown>) => Promise<any>; close: () => void };

export async function connectAi(vault: string, token: string): Promise<Ai> {
	const child = spawn(IPSE, ['mcp', '--vault', vault], {
		env: { ...process.env, IPSE_TOKEN: token },
		stdio: ['pipe', 'pipe', 'ignore']
	});
	const waiting = new Map<number, (message: any) => void>();
	let buffer = '';
	child.stdout!.on('data', (chunk: Buffer) => {
		buffer += chunk.toString();
		let end: number;
		while ((end = buffer.indexOf('\n')) >= 0) {
			const line = buffer.slice(0, end).trim();
			buffer = buffer.slice(end + 1);
			if (!line) continue;
			const message = JSON.parse(line);
			if (message.id !== undefined) waiting.get(message.id)?.(message);
		}
	});
	let next = 0;
	const send = (method: string, params: unknown) =>
		new Promise<any>((done, fail) => {
			const id = ++next;
			waiting.set(id, (message) => (message.error ? fail(new Error(message.error.message)) : done(message.result)));
			child.stdin!.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
		});
	await send('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'tests', version: '1' } });
	child.stdin!.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
	return {
		async call(tool, args = {}) {
			const result = await send('tools/call', { name: tool, arguments: args });
			if (result.isError) throw new Error(result.content.map((c: { text: string }) => c.text).join(''));
			return result.structuredContent;
		},
		close: () => child.kill()
	};
}
