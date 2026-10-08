// Parses each input of the corpus (first argument) with the official packages and compares the nodes with
// those mq wrote (second argument). Both are put in the same flat form first: the kinds and values of the
// nodes in document order, with containers opened by `kind(` and closed by `)`. Paragraphs are not
// marked, since mq has none, and the items of nested lists follow their parent item, as in mq.
// Differences must be listed in known.json by the id of the input.
import fs from 'node:fs'
import {fromMarkdown} from 'mdast-util-from-markdown'
import {frontmatterFromMarkdown} from 'mdast-util-frontmatter'
import {gfmFromMarkdown} from 'mdast-util-gfm'
import {mathFromMarkdown} from 'mdast-util-math'
import {mdxFromMarkdown} from 'mdast-util-mdx'
import {frontmatter} from 'micromark-extension-frontmatter'
import {gfm} from 'micromark-extension-gfm'
import {math} from 'micromark-extension-math'
import {mdxjs} from 'micromark-extension-mdxjs'

const read = (file) => JSON.parse(fs.readFileSync(file, 'utf8'))
const corpus = read(process.argv[2])
const actual = read(process.argv[3])
const known = read(new URL('known.json', import.meta.url))

const json = JSON.stringify
const text = (value) => `T${json(value)}`

/** Joins adjacent text tokens. */
function merge(tokens) {
  const out = []
  for (const token of tokens) {
    if (token.startsWith('T') && out.length && out.at(-1).startsWith('T')) {
      out[out.length - 1] = text(JSON.parse(out.at(-1).slice(1)) + JSON.parse(token.slice(1)))
    } else {
      out.push(token)
    }
  }
  return out
}

const container = (head, inner) => [`${head}(`, ...inner, ')']
const mdxAttribute = (attribute) =>
  attribute.type === 'mdxJsxExpressionAttribute'
    ? `...{${attribute.value}}`
    : attribute.value == null
      ? attribute.name
      : typeof attribute.value === 'string'
        ? `${attribute.name}=lit:${json(attribute.value)}`
        : `${attribute.name}=expr:{${attribute.value.value}}`
const item = (level, ordered, checked, spread) =>
  `item ${level} ${ordered === null ? 'u' : `o${ordered}`} ${checked === true ? 'x' : checked === false ? '_' : '-'} ${spread ? 's' : 't'}`
const alignment = (align) => `align ${json(align.map((a) => a ?? 'none'))}`

function fromMdast(node, level = 0) {
  const kids = (inner = true) => {
    const tokens = (node.children ?? []).flatMap((child) => fromMdast(child, level))
    return inner ? merge(tokens) : tokens
  }
  switch (node.type) {
    case 'root': return kids(false)
    case 'paragraph': return kids()
    case 'heading': return container(`heading ${node.depth}`, kids())
    case 'thematicBreak': return ['hr']
    case 'code': return [`code ${node.lang ?? ''} ${node.meta ?? ''} ${json(node.value)}`]
    case 'html': return [`html ${json(node.value)}`]
    case 'blockquote': return container('blockquote', kids(false))
    case 'list':
      return node.children.flatMap((listItem, index) => {
        const nested = []
        const body = listItem.children.flatMap((child) =>
          child.type === 'list' ? (nested.push(...fromMdast(child, level + 1)), []) : fromMdast(child, level),
        )
        const number = node.ordered ? (node.start ?? 1) + index : null
        return [...container(item(level, number, listItem.checked, node.spread), body), ...nested]
      })
    case 'table':
      return node.children.flatMap((row, r) => [
        ...row.children.flatMap((cell, c) => container(`cell ${r} ${c}`, merge(cell.children.flatMap((n) => fromMdast(n))))),
        ...(r === 0 ? [alignment(node.align)] : []),
      ])
    case 'definition': return [`definition ${node.identifier} ${node.url} ${node.title ?? ''}`]
    case 'footnoteDefinition': return container(`footnote ${node.identifier}`, kids(false))
    case 'footnoteReference': return [`footnoteref ${node.identifier}`]
    case 'math': return [`math ${json(node.value)}`]
    case 'inlineMath': return [`imath ${json(node.value)}`]
    case 'yaml': return [`yaml ${json(node.value)}`]
    case 'toml': return [`toml ${json(node.value)}`]
    case 'text': return [text(node.value)]
    case 'emphasis': return container('em', kids())
    case 'strong': return container('strong', kids())
    case 'delete': return container('del', kids())
    case 'inlineCode': return [`code_inline ${json(node.value)}`]
    case 'break': return ['br']
    case 'link': return container(`link ${node.url} ${node.title ?? ''}`, kids())
    case 'image': return [`image ${node.url} ${node.title ?? ''} ${json(node.alt)}`]
    case 'linkReference': return container(`linkref ${node.identifier}`, kids())
    case 'imageReference': return [`imageref ${node.identifier} ${json(node.alt)}`]
    case 'mdxJsxFlowElement':
    case 'mdxJsxTextElement': {
      const kind = node.type === 'mdxJsxFlowElement' ? 'flow' : 'text'
      const attributes = node.attributes.map((a) => ' ' + mdxAttribute(a)).join('')
      return container(`${kind} <${node.name ?? ''}${attributes}>`, kids(node.type === 'mdxJsxTextElement'))
    }
    case 'mdxFlowExpression': return [`flowexpr {${node.value}}`]
    case 'mdxTextExpression': return [`textexpr {${node.value}}`]
    case 'mdxjsEsm': return [`esm ${json(node.value)}`]
    default: throw new Error(`unknown mdast node ${node.type}`)
  }
}

function mqAttribute(attribute) {
  if (attribute.type === 'expression') return `...{${attribute.value}}`
  const {name, value} = attribute.value
  if (value === null) return name
  return value.type === 'literal' ? `${name}=lit:${json(value.value)}` : `${name}=expr:{${value.value}}`
}

function fromMq(node) {
  const kids = (inner = true) => {
    const tokens = (node.values ?? node.children ?? []).flatMap(fromMq)
    return inner ? merge(tokens) : tokens
  }
  switch (node.type) {
    case 'Heading': return container(`heading ${node.depth}`, kids())
    case 'HorizontalRule': return ['hr']
    case 'Code': return [`code ${node.lang ?? ''} ${node.meta ?? ''} ${json(node.value)}`]
    case 'Html': return [`html ${json(node.value)}`]
    case 'Blockquote': return container('blockquote', kids(false))
    case 'List': {
      const number = node.ordered ? (node.start ?? 1) + node.index : null
      return container(item(node.level, number, node.checked, node.spread), kids(false))
    }
    case 'TableCell': return container(`cell ${node.row} ${node.column}`, kids())
    case 'TableAlign': return [alignment(node.align)]
    case 'Definition': return [`definition ${node.ident} ${node.url} ${node.title ?? ''}`]
    case 'Footnote': return container(`footnote ${node.ident}`, kids(false))
    case 'FootnoteRef': return [`footnoteref ${node.ident}`]
    case 'Math': return [`math ${json(node.value)}`]
    case 'MathInline': return [`imath ${json(node.value)}`]
    case 'Yaml': return [`yaml ${json(node.value)}`]
    case 'Toml': return [`toml ${json(node.value)}`]
    case 'Text': return [text(node.value)]
    case 'Emphasis': return container('em', kids())
    case 'Strong': return container('strong', kids())
    case 'Delete': return container('del', kids())
    case 'CodeInline': return [`code_inline ${json(node.value)}`]
    case 'Break': return ['br']
    case 'Link': return container(`link ${node.url} ${node.title ?? ''}`, kids())
    case 'Image': return [`image ${node.url} ${node.title ?? ''} ${json(node.alt)}`]
    case 'LinkRef': return container(`linkref ${node.ident}`, kids())
    case 'ImageRef': return [`imageref ${node.ident} ${json(node.alt)}`]
    case 'MdxJsxFlowElement':
    case 'MdxJsxTextElement': {
      const kind = node.type === 'MdxJsxFlowElement' ? 'flow' : 'text'
      const attributes = node.attributes.map((a) => ' ' + mqAttribute(a)).join('')
      return container(`${kind} <${node.name ?? ''}${attributes}>`, kids(node.type === 'MdxJsxTextElement'))
    }
    case 'MdxFlowExpression': return [`flowexpr {${node.value}}`]
    case 'MdxTextExpression': return [`textexpr {${node.value}}`]
    case 'MdxJsEsm': return [`esm ${json(node.value)}`]
    case 'Empty': return []
    default: throw new Error(`unknown mq node ${node.type}`)
  }
}

const OPTIONS = {
  markdown: {
    extensions: [gfm(), frontmatter(['yaml', 'toml']), math()],
    mdastExtensions: [gfmFromMarkdown(), frontmatterFromMarkdown(['yaml', 'toml']), mathFromMarkdown()],
  },
  mdx: {extensions: [mdxjs()], mdastExtensions: [mdxFromMarkdown()]},
}

function official({mode, input}) {
  try {
    return {error: null, tokens: fromMdast(fromMarkdown(input, OPTIONS[mode]))}
  } catch (error) {
    if (error.message.startsWith('unknown mdast node')) throw error
    return {error: String(error.reason ?? error.message).split('\n')[0], tokens: []}
  }
}

const unexpected = []
const fixed = []
let differing = 0
corpus.forEach((entry, index) => {
  const expected = official(entry)
  const mq = actual[index]
  const got = mq.error === null ? {error: null, tokens: mq.nodes.flatMap(fromMq)} : {error: mq.error, tokens: []}
  const same =
    (expected.error === null) === (got.error === null) &&
    (expected.error !== null || json(expected.tokens) === json(got.tokens))
  if (!same) differing++
  if (!same && !(entry.id in known)) {
    const at = expected.tokens.findIndex((token, i) => token !== got.tokens[i])
    unexpected.push(
      `${entry.id} ${json(entry.input)}\n  official: ${expected.error ?? json(expected.tokens.slice(Math.max(at, 0)))}\n  mq:       ${got.error ?? json(got.tokens.slice(Math.max(at, 0)))}`,
    )
  } else if (same && entry.id in known) {
    fixed.push(entry.id)
  }
})

const ids = new Set(corpus.map((entry) => entry.id))
for (const id of Object.keys(known)) {
  if (!ids.has(id)) unexpected.push(`known.json has an id that is not in the corpus: ${id}`)
}
if (unexpected.length) console.error(`differs from the official packages:\n${unexpected.join('\n')}`)
if (fixed.length) console.error(`now the same, remove from known.json:\n${fixed.join('\n')}`)
console.log(`${corpus.length} inputs, ${differing} differ, ${Object.keys(known).length} known`)
process.exit(unexpected.length || fixed.length ? 1 : 0)
