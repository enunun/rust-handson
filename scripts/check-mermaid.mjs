// Gitが管理するすべてのMarkdownから```mermaidのブロックを取り出し，構文を検査する．
// 使い方: node scripts/check-mermaid.mjs [ファイル...]  (省略するとgit ls-files '*.md')
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import { JSDOM } from 'jsdom';

// mermaidは読み込み時にDOMを必要とするため，先にjsdomで用意する．
const dom = new JSDOM('<!doctype html><body></body>');
globalThis.window = dom.window;
globalThis.document = dom.window.document;
const { default: mermaid } = await import('mermaid');
mermaid.initialize({ startOnLoad: false });

const files = process.argv.length > 2
	? process.argv.slice(2)
	: execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '*.md'], { encoding: 'utf8' })
		.split('\n')
		.filter((f) => f && fs.existsSync(f));

let blocks = 0;
let failures = 0;
for (const file of files) {
	const lines = fs.readFileSync(file, 'utf8').split('\n');
	for (let i = 0; i < lines.length; i++) {
		if (lines[i].trim() !== '```mermaid') continue;
		const start = i + 1;
		const body = [];
		for (i = start; i < lines.length && lines[i].trim() !== '```'; i++) body.push(lines[i]);
		blocks++;
		try {
			await mermaid.parse(body.join('\n'));
		} catch (e) {
			failures++;
			console.error(`${file}:${start}: ${String(e.message ?? e).split('\n').slice(0, 3).join(' ')}`);
		}
	}
}

console.log(`mermaid: ${blocks} blocks, ${failures} errors`);
process.exit(failures === 0 ? 0 : 1);
