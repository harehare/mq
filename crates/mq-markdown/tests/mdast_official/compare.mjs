// Parses each input of corpus.json with the official MDX packages and compares the result with the
// nodes mq wrote to the file in the first argument. Differences must be listed in known.json.
import fs from 'node:fs'
import {fromMarkdown} from 'mdast-util-from-markdown'
import {mdxFromMarkdown} from 'mdast-util-mdx'
import {mdxjs} from 'micromark-extension-mdxjs'

const dir = new URL('.', import.meta.url)
const read = (file) => JSON.parse(fs.readFileSync(file, 'utf8'))
const corpus = read(new URL('corpus.json', dir))
const known = read(new URL('known.json', dir))
const mq = read(process.argv[2])

const attribute = (a) =>
  a.type === 'mdxJsxExpressionAttribute'
    ? `...{${a.value}}`
    : a.value == null
      ? a.name
      : typeof a.value === 'string'
        ? `${a.name}=lit:${JSON.stringify(a.value)}`
        : `${a.name}=expr:{${a.value.value}}`

const children = (node) => (node.children ?? []).flatMap(normalize)

// The same flat description as `tests/mdx_official.rs` writes: MDX nodes with their children, and the
// text, code and HTML around them. Other nodes are looked through.
function normalize(node) {
  switch (node.type) {
    case 'mdxJsxFlowElement':
    case 'mdxJsxTextElement': {
      const kind = node.type === 'mdxJsxFlowElement' ? 'flow' : 'text'
      const attributes = node.attributes.map((a) => ' ' + attribute(a)).join('')
      return [`${kind} <${node.name ?? ''}${attributes}>(`, ...children(node), ')']
    }
    case 'mdxFlowExpression': return [`flowexpr {${node.value}}`]
    case 'mdxTextExpression': return [`textexpr {${node.value}}`]
    case 'mdxjsEsm': return [`esm ${JSON.stringify(node.value)}`]
    case 'text': case 'inlineCode': case 'code': return [`T${JSON.stringify(node.value)}`]
    case 'html': return [`html ${JSON.stringify(node.value)}`]
    case 'yaml': return ['yaml']
    case 'thematicBreak': return ['hr']
    default: return children(node)
  }
}

// Adjacent text is joined and whitespace collapsed, as the node structure of paragraphs differs.
function merge(items) {
  const out = []
  for (const item of items) {
    if (!item.startsWith('T')) {
      out.push(item)
      continue
    }
    const text = JSON.parse(item.slice(1)).replace(/\s+/g, ' ').trim()
    if (!text) continue
    if (out.length && out.at(-1).startsWith('T')) {
      out[out.length - 1] = 'T' + JSON.stringify(JSON.parse(out.at(-1).slice(1)) + text)
    } else {
      out.push('T' + JSON.stringify(text))
    }
  }
  return out
}

const official = (input) => {
  try {
    const tree = fromMarkdown(input, {extensions: [mdxjs()], mdastExtensions: [mdxFromMarkdown()]})
    return {error: null, items: merge(children(tree))}
  } catch (error) {
    return {error: String(error.reason ?? error.message).split('\n')[0], items: []}
  }
}

const unexpected = []
const fixed = []
corpus.forEach((input, index) => {
  const expected = official(input)
  const actual = {error: mq[index].error, items: merge(mq[index].items)}
  const same =
    (expected.error === null) === (actual.error === null) &&
    (expected.error !== null || JSON.stringify(expected.items) === JSON.stringify(actual.items))
  if (!same && !(input in known)) {
    unexpected.push(`${JSON.stringify(input)}\n  official: ${expected.error ?? JSON.stringify(expected.items)}\n  mq:       ${actual.error ?? JSON.stringify(actual.items)}`)
  } else if (same && input in known) {
    fixed.push(JSON.stringify(input))
  }
})

for (const input of Object.keys(known)) {
  if (!corpus.includes(input)) unexpected.push(`known.json has an input that is not in corpus.json: ${JSON.stringify(input)}`)
}
if (unexpected.length) console.error(`differs from the official MDX:\n${unexpected.join('\n')}`)
if (fixed.length) console.error(`now the same, remove from known.json:\n${fixed.join('\n')}`)
console.log(`${corpus.length} inputs, ${Object.keys(known).length} known differences`)
process.exit(unexpected.length || fixed.length ? 1 : 0)
