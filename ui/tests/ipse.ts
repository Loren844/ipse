// Starts `ipse ui --no-open` on a new invented memory and gives its one-time address.
import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const exe = process.platform === 'win32' ? 'ipse.exe' : 'ipse';
export const IPSE = process.env.IPSE_BIN ?? resolve(import.meta.dirname, '../../target/debug', exe);

export type Running = { url: string; origin: string; startedAt: number; stop: () => void };

export async function startIpse(name = 'Inès Roux', language = 'fr'): Promise<Running> {
	const dir = mkdtempSync(join(tmpdir(), 'ipse-ui-'));
	const vault = join(dir, 'memoire');
	execFileSync(IPSE, ['init', vault, '--name', name, '--language', language, '--time-zone', 'Europe/Paris']);
	const startedAt = Date.now();
	const child: ChildProcess = spawn(IPSE, ['ui', '--no-open', '--vault', vault], { stdio: ['ignore', 'pipe', 'inherit'] });
	const url = await new Promise<string>((ok, fail) => {
		let seen = '';
		child.stdout!.on('data', (chunk: Buffer) => {
			seen += chunk.toString();
			const found = seen.match(/http:\/\/127\.0\.0\.1:\d+\/login\?code=[0-9a-f]+/);
			if (found) ok(found[0]);
		});
		child.on('exit', () => fail(new Error(`ipse ui stopped: ${seen}`)));
	});
	return {
		url,
		origin: new URL(url).origin,
		startedAt,
		stop: () => {
			child.kill();
			rmSync(dir, { recursive: true, force: true });
		}
	};
}
