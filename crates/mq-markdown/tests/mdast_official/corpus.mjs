// Writes the inputs to compare to the file in the first argument: the hand-written corpora and the
// examples of the specs, which are fetched over the network.
import fs from 'node:fs'

const dir = new URL('.', import.meta.url)
const read = (file) => JSON.parse(fs.readFileSync(new URL(file, dir), 'utf8'))

const SPECS = [
  ['commonmark', 'https://raw.githubusercontent.com/commonmark/commonmark-spec/0.31.2/spec.txt'],
  ['gfm', 'https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/spec.txt'],
  ['gfm-extensions', 'https://raw.githubusercontent.com/github/cmark-gfm/828322d1ee4facdab56f0d3edccb13e9af90dcd2/test/extensions.txt'],
]

const fence = '`'.repeat(32)

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
  const response = await fetch(url)
  if (!response.ok) throw new Error(`${url}: ${response.status}`)
  examples(await response.text()).forEach((input, index) => {
    corpus.push({id: `${name}#${index + 1}`, mode: 'markdown', input})
  })
}
fs.writeFileSync(process.argv[2], JSON.stringify(corpus))
console.log(`${corpus.length} inputs`)
