// 模範解答の設計ドキュメントを実装と照合する．
//
// - design/c4-component.md: Componentとモジュール，Relとモジュールの間の参照が一致するか．
// - design/code-types.md: classの名前が，src/のstruct，enum，trait，typeとして定義されているか．
//
// 使い方: node scripts/check-design.mjs [パッケージのディレクトリ...]
//         (省略するとiterations/*/solution)
import fs from 'node:fs';
import path from 'node:path';

const packages = process.argv.length > 2
	? process.argv.slice(2)
	: (fs.existsSync('iterations') ? fs.readdirSync('iterations', { withFileTypes: true }) : [])
		.filter((d) => d.isDirectory())
		.map((d) => path.join('iterations', d.name, 'solution'))
		.filter((p) => fs.existsSync(path.join(p, 'src')));

// src/の下のファイルをモジュールのパスに対応付ける．lib.rsは"crate"，main.rsは"main"．
function collectModules(srcDir) {
	const modules = new Map();
	const walk = (dir, prefix) => {
		for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
			const full = path.join(dir, entry.name);
			if (entry.isDirectory()) {
				walk(full, [...prefix, entry.name]);
			} else if (entry.name.endsWith('.rs')) {
				const stem = entry.name.slice(0, -3);
				let segments;
				if (prefix.length === 0 && stem === 'lib') segments = [];
				else if (prefix.length === 0 && stem === 'main') segments = null;
				else if (stem === 'mod') segments = prefix;
				else segments = [...prefix, stem];
				const label = segments === null ? 'main' : segments.length === 0 ? 'crate' : segments.join('::');
				modules.set(label, { file: full, segments });
			}
		}
	};
	walk(srcDir, []);
	return modules;
}

// `mod`の宣言だけを持つモジュール(子モジュールをまとめる名前空間)かどうか．
// 名前空間は図のComponentにせず，Container_Boundaryなどでまとめて描く．
function isNamespace(module) {
	return productionCode(module.file)
		.split('\n')
		.every((line) => /^\s*((pub(\([^)]*\))?\s+)?mod\s+\w+;)?\s*$/.test(line));
}

// コメントと，#[cfg(test)]から後ろ(単体テスト)を除いたコードを返す．
function productionCode(file) {
	let text = fs.readFileSync(file, 'utf8');
	const testStart = text.indexOf('#[cfg(test)]');
	if (testStart >= 0) text = text.slice(0, testStart);
	return text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
}

// useの木(`a::{b, c::d as e}`)を，パスの一覧に展開する．
function expandUseTree(tree, prefix = []) {
	tree = tree.trim();
	const brace = tree.indexOf('{');
	if (brace < 0) {
		const segments = tree.replace(/\s+as\s+\w+$/, '').split('::').map((s) => s.trim()).filter(Boolean);
		const full = [...prefix, ...segments];
		if (full.at(-1) === 'self' || full.at(-1) === '*') full.pop();
		return [full];
	}
	const head = tree.slice(0, brace).replace(/::\s*$/, '');
	const inner = tree.slice(brace + 1, tree.lastIndexOf('}'));
	const nextPrefix = [...prefix, ...head.split('::').map((s) => s.trim()).filter(Boolean)];
	const parts = [];
	let depth = 0;
	let current = '';
	for (const ch of inner) {
		if (ch === '{') depth++;
		if (ch === '}') depth--;
		if (ch === ',' && depth === 0) {
			parts.push(current);
			current = '';
		} else {
			current += ch;
		}
	}
	parts.push(current);
	return parts.filter((p) => p.trim()).flatMap((p) => expandUseTree(p, nextPrefix));
}

// モジュールfromから見たパスを，参照先のモジュールのラベルに解決する．外部のクレートならnull．
function resolve(pathSegments, from, modules) {
	const isModule = (segs) => [...modules.values()].some((m) => m.segments && m.segments.join('::') === segs.join('::'));
	let absolute;
	const [first, ...rest] = pathSegments;
	if (first === 'crate' || (first === 'ferrodb' && from.segments === null)) {
		absolute = rest;
	} else if (first === 'super' || first === 'self') {
		let base = [...from.segments];
		let segs = pathSegments;
		while (segs[0] === 'super') {
			base.pop();
			segs = segs.slice(1);
		}
		if (segs[0] === 'self') segs = segs.slice(1);
		absolute = [...base, ...segs];
	} else if (from.segments !== null && isModule([...from.segments, first])) {
		absolute = [...from.segments, ...pathSegments];
	} else {
		return null;
	}
	for (let n = absolute.length; n > 0; n--) {
		if (isModule(absolute.slice(0, n))) return absolute.slice(0, n).join('::');
	}
	return 'crate';
}

function codeDependencies(modules) {
	const edges = new Set();
	for (const [label, module] of modules) {
		let code = productionCode(module.file);
		const paths = [];
		code = code.replace(/\buse\s+([^;]+);/g, (_, tree) => {
			paths.push(...expandUseTree(tree));
			return '';
		});
		// crate::a::f()やsuper::f()のようなパス式と，子モジュールの名前で始まるa::f()のようなパス式．
		// 子モジュールでない名前(std::やToken::など)で始まるパスは，resolveが外部として除く．
		for (const m of code.matchAll(/(?<![\w:])([A-Za-z_]\w*)((?:::[A-Za-z_]\w*)+)/g)) {
			paths.push([m[1], ...m[2].split('::').filter(Boolean)]);
		}
		for (const p of paths) {
			const target = resolve(p, module, modules);
			if (target && target !== label) edges.add(`${label} -> ${target}`);
		}
	}
	return edges;
}

function mermaidBlocks(file) {
	return [...fs.readFileSync(file, 'utf8').matchAll(/```mermaid\n([\s\S]*?)```/g)].map((m) => m[1]);
}

function diagramComponents(file) {
	const aliases = new Map();
	const edges = new Set();
	const text = mermaidBlocks(file).join('\n');
	for (const m of text.matchAll(/\bComponent\(\s*(\w+)\s*,\s*"([^"]+)"/g)) aliases.set(m[1], m[2]);
	for (const m of text.matchAll(/\b(?:Rel|Rel_[A-Za-z]+)\(\s*(\w+)\s*,\s*(\w+)/g)) {
		if (aliases.has(m[1]) && aliases.has(m[2])) edges.add(`${aliases.get(m[1])} -> ${aliases.get(m[2])}`);
	}
	return { labels: new Set(aliases.values()), edges };
}

function diagramClasses(file) {
	const names = new Set();
	for (const block of mermaidBlocks(file)) {
		for (const m of block.matchAll(/^\s*class\s+([A-Za-z_]\w*)/gm)) names.add(m[1]);
	}
	return names;
}

function definedTypes(modules) {
	const names = new Set();
	for (const module of modules.values()) {
		const code = productionCode(module.file);
		for (const m of code.matchAll(/\b(?:struct|enum|trait|type)\s+([A-Za-z_]\w*)/g)) names.add(m[1]);
	}
	return names;
}

const difference = (a, b) => [...a].filter((x) => !b.has(x)).sort();

let problems = 0;
const report = (pkg, message, items) => {
	if (items.length === 0) return;
	problems += items.length;
	console.error(`${pkg}: ${message}`);
	for (const item of items) console.error(`  ${item}`);
};

for (const pkg of packages) {
	const modules = collectModules(path.join(pkg, 'src'));

	const componentFile = path.join(pkg, 'design', 'c4-component.md');
	if (fs.existsSync(componentFile)) {
		const diagram = diagramComponents(componentFile);
		const codeLabels = new Set([...modules].filter(([, module]) => !isNamespace(module)).map(([label]) => label));
		const codeEdges = codeDependencies(modules);
		report(pkg, 'Component図にないモジュール', difference(codeLabels, diagram.labels));
		report(pkg, 'コードにないComponent', difference(diagram.labels, codeLabels));
		report(pkg, 'Component図にない依存', difference(codeEdges, diagram.edges));
		report(pkg, 'コードにない依存(Rel)', difference(diagram.edges, codeEdges));
	}

	const typesFile = path.join(pkg, 'design', 'code-types.md');
	if (fs.existsSync(typesFile)) {
		report(pkg, 'コードで定義されていないclass', difference(diagramClasses(typesFile), definedTypes(modules)));
	}
}

console.log(`design: ${packages.length} packages, ${problems} problems`);
process.exit(problems === 0 ? 0 : 1);
