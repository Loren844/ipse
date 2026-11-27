// Starts `ipse ui --no-open` on a new invented memory and gives its one-time address.
import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const exe = process.platform === 'win32' ? 'ipse.exe' : 'ipse';
export const IPSE = process.env.IPSE_BIN ?? resolve(import.meta.dirname, '../../target/debug', exe);

/** `startup`: milliseconds from starting `ipse ui` to its address being printed;
 * `vault`: the memory folder, to check what the interface wrote. */
export type Running = { url: string; origin: string; vault: string; home: string; startup: number; stop: () => void };

const FIXTURES = resolve(import.meta.dirname, '../../crates/ipse/tests/fixtures');

/** Starts `ipse ui` on a new memory; with `imported`, the test contacts and calendar in
 * it; `prepare` completes the memory folder before `ipse ui` starts. */
export async function startIpse(
	name = 'Inès Roux',
	language = 'fr',
	imported = false,
	prepare: (vault: string) => void = () => {}
): Promise<Running> {
	const dir = mkdtempSync(join(tmpdir(), 'ipse-ui-'));
	const vault = join(dir, 'memoire');
	execFileSync(IPSE, ['init', vault, '--name', name, '--language', language, '--time-zone', 'Europe/Paris']);
	if (imported) {
		for (const file of ['contacts/google-v3.vcf', 'calendar/google.ics'])
			execFileSync(IPSE, ['import', join(FIXTURES, file), '--vault', vault]);
	}
	prepare(vault);
	return serve(dir, vault);
}

/** Starts `ipse ui` on the demo memory (`ipse init --demo`): about 150 invented people,
 * two years of agenda, summaries and proposals waiting. */
export async function startDemo(language = 'fr'): Promise<Running> {
	const dir = mkdtempSync(join(tmpdir(), 'ipse-ui-'));
	const vault = join(dir, 'memoire');
	execFileSync(IPSE, ['init', vault, '--demo', '--language', language, '--time-zone', 'Europe/Paris']);
	return serve(dir, vault);
}

async function serve(dir: string, vault: string): Promise<Running> {
	const startedAt = Date.now();
	// A home of its own: what Ipse writes into AI apps' configurations stays in the test.
	const home = join(dir, 'home');
	mkdirSync(home, { recursive: true });
	// The disk's encryption as this machine's would be unknown from one runner to the
	// next: on, unless a test says otherwise (issue #262).
	const env = {
		...process.env,
		HOME: home,
		USERPROFILE: home,
		APPDATA: join(home, 'AppData', 'Roaming'),
		IPSE_DISK_ENCRYPTION: process.env.IPSE_DISK_ENCRYPTION ?? 'on'
	};
	const child: ChildProcess = spawn(IPSE, ['ui', '--no-open', '--vault', vault], { stdio: ['ignore', 'pipe', 'inherit'], env });
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
		vault,
		home,
		startup: Date.now() - startedAt,
		stop: () => {
			child.kill();
			rmSync(dir, { recursive: true, force: true });
		}
	};
}
