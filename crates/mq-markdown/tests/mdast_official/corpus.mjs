// Writes the inputs to compare to the file in the first argument: the hand-written corpora and the
// examples of the specs, which are fetched over the network.
import fs from 'node:fs'
import {generate} from './generate.mjs'

const dir = new URL('.', import.meta.url)
const read = (file) => JSON.parse(fs.readFileSync(new URL(file, dir), 'utf8'))

const SPECS = [
  ['commonmark', 'https://raw.githubusercontent.com/commonmark/commonmark-spec/0.31.2/spec.txt'],
  ['gfm', 'https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/spec.txt'],
  ['gfm-extensions', 'https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/extensions.txt'],
]

const MARKDOWN_RS = 'https://raw.githubusercontent.com/wooorm/markdown-rs/1.0.0/tests'
// Test files whose inputs are read as MDX; the others are read as Markdown. `mdx_swc` needs a JavaScript parser.
const MDX_TESTS = ['mdx_esm', 'mdx_expression_flow', 'mdx_expression_text', 'mdx_jsx_flow', 'mdx_jsx_text']
const MARKDOWN_RS_TESTS = [
  ...MDX_TESTS,
  'attention', 'autolink', 'block_quote', 'character_escape', 'character_reference', 'code_fenced',
  'code_indented', 'code_text', 'commonmark', 'definition', 'frontmatter', 'gfm_autolink_literal',
  'gfm_footnote', 'gfm_strikethrough', 'gfm_table', 'gfm_tagfilter', 'gfm_task_list_item',
  'hard_break_escape', 'hard_break_trailing', 'heading_atx', 'heading_setext', 'html_flow', 'html_text',
  'image', 'link_reference', 'link_resource', 'list', 'math_flow', 'math_text', 'misc_bom',
  'misc_dangerous_html', 'misc_dangerous_protocol', 'misc_line_ending', 'misc_soft_break', 'misc_tabs',
  'misc_url', 'misc_zero', 'text', 'thematic_break',
]
const MICROMARK_TESTS = [
  'https://raw.githubusercontent.com/micromark/micromark-extension-mdx-jsx/3.0.2/test/index.js',
  'https://raw.githubusercontent.com/micromark/micromark-extension-mdx-expression/micromark-extension-mdx-expression%403.0.1/test/index.js',
  'https://raw.githubusercontent.com/micromark/micromark-extension-mdxjs-esm/3.0.0/test/index.js',
]

const fence = '`'.repeat(32)

const ESCAPES = {n: '\n', r: '\r', t: '\t', 0: '\0', '\\': '\\', '"': '"', "'": "'", '`': '`', $: '$'}

/** The string literal that starts at `index` in Rust or JavaScript source, or null. */
function literal(source, index) {
  const quote = source[index]
  if (!['"', "'", '`'].includes(quote)) return null
  let value = ''
  for (let i = index + 1; i < source.length; i++) {
    const char = source[i]
    if (char === quote) return value
    if (char !== '\\') {
      value += char
      continue
    }
    const next = source[++i]
    if (next === '\n') {
      // A Rust line continuation drops the whitespace that follows.
      while (/\s/.test(source[i + 1] ?? '')) i++
    } else if (next === 'u' && source[i + 1] === '{') {
      const end = source.indexOf('}', i)
      value += String.fromCodePoint(parseInt(source.slice(i + 2, end), 16))
      i = end
    } else if (next === 'u' || next === 'x') {
      const length = next === 'u' ? 4 : 2
      value += String.fromCodePoint(parseInt(source.slice(i + 1, i + 1 + length), 16))
      i += length
    } else if (next in ESCAPES) {
      value += ESCAPES[next]
    } else {
      return null
    }
  }
  return null
}

/** The first argument, when it is a string literal, of each call to one of `names` in `source`. */
function firstArguments(source, names) {
  const out = []
  const pattern = new RegExp(`\\b(?:${names.join('|')})\\(\\s*(?=["'\`])`, 'g')
  for (const match of source.matchAll(pattern)) {
    const value = literal(source, match.index + match[0].length)
    if (value !== null) out.push(value)
  }
  return out
}

async function fetchText(url) {
  const response = await fetch(url)
  if (!response.ok) throw new Error(`${url}: ${response.status}`)
  return response.text()
}

/** The markdown of each example of a spec, numbered from 1 as in the specs. */
function examples(text) {
  const lines = text.split('\n')
  const out = []
  for (let i = 0; i < lines.length; i++) {
    if (lines[i] !== `${fence} example`) continue
    const markdown = []
    for (i++; lines[i] !== '.'; i++) markdown.push(lines[i])
    while (lines[i] !== fence) i++
    out.push(markdown.join('\n').replaceAll('→', '\t') + '\n')
  }
  return out
}

const corpus = []
for (const input of read('corpus-mdx.json')) corpus.push({id: `mdx:${input}`, mode: 'mdx', input})
for (const input of read('corpus-markdown.json')) corpus.push({id: `markdown:${input}`, mode: 'markdown', input})
for (const [name, url] of SPECS) {
  examples(await fetchText(url)).forEach((input, index) => {
    corpus.push({id: `${name}#${index + 1}`, mode: 'markdown', input})
  })
}

const seen = new Set(corpus.map((entry) => `${entry.mode}\0${entry.input}`))
function addTests(name, mode, inputs) {
  let count = 0
  for (const input of inputs) {
    if (input.length > 5000 || !input.isWellFormed() || seen.has(`${mode}\0${input}`)) continue
    seen.add(`${mode}\0${input}`)
    corpus.push({id: `${name}#${++count}`, mode, input})
  }
}
for (const name of MARKDOWN_RS_TESTS) {
  const source = await fetchText(`${MARKDOWN_RS}/${name}.rs`)
  const mode = MDX_TESTS.includes(name) ? 'mdx' : 'markdown'
  addTests(`markdown-rs/${name}`, mode, firstArguments(source, ['to_html', 'to_html_with_options', 'to_mdast']))
}
for (const url of MICROMARK_TESTS) {
  const name = url.split('/').slice(-4, -2).join('/').replace('%40', '@')
  addTests(name, 'mdx', firstArguments(await fetchText(url), ['micromark']))
}
// Random documents. MDAST_RANDOM is how many of each kind, MDAST_SEED the seed.
const random = Number(process.env.MDAST_RANDOM ?? 500)
const seed = Number(process.env.MDAST_SEED ?? 1)
for (const [mode, offset] of [['markdown', 0], ['mdx', 1]]) {
  generate(random, seed + offset, mode === 'mdx').forEach((input, index) => {
    corpus.push({id: `random-${mode}-${seed}#${index + 1}`, mode, input})
  })
}
fs.writeFileSync(process.argv[2], JSON.stringify(corpus))
console.log(`${corpus.length} inputs`)
