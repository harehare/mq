// Random documents built from a grammar of Markdown and MDX constructs. The same seed makes the same
// documents, so the ids of the inputs stay the same and known.json can list them.

/** A small seeded generator (mulberry32). */
function random(seed) {
  let state = seed >>> 0
  return () => {
    state = (state + 0x6d2b79f5) >>> 0
    let t = state
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const WORDS = ['a', 'bc', 'foo', 'x1', 'Zed', 'été', '日本', 'a_b', '1', 'q']

export function generate(count, seed, mdx) {
  const next = random(seed)
  const pick = (list) => list[Math.floor(next() * list.length)]
  const chance = (p) => next() < p
  const word = () => pick(WORDS)
  const words = (max = 4) => Array.from({length: 1 + Math.floor(next() * max)}, word).join(' ')

  const inline = (depth = 0) => {
    const parts = []
    for (let i = 0, n = 1 + Math.floor(next() * 3); i < n; i++) {
      const choices = [
        () => words(),
        () => `*${depth < 2 ? inline(depth + 1) : word()}*`,
        () => `**${depth < 2 ? inline(depth + 1) : word()}**`,
        () => `\`${word()}\``,
        () => `[${word()}](/${word()}${chance(0.3) ? ` "${word()}"` : ''})`,
        () => `![${word()}](/${word()}.png)`,
        () => `[${word()}][r${Math.floor(next() * 2)}]`,
        () => `${word()}  \n${word()}`,
        () => `\\${pick(['*', '_', '[', '#', '`'])}${word()}`,
        () => `&${pick(['amp', 'lt', 'copy'])};`,
      ]
      if (!mdx) {
        choices.push(
          () => `~~${word()}~~`,
          () => `$${word()}$`,
          () => `[^f${Math.floor(next() * 2)}]`,
          () => `<${pick(['b', 'span'])}>${word()}</${pick(['b', 'span'])}>`,
        )
      } else {
        choices.push(
          () => `<${pick(['B', 'a.b', 'x:y'])}${attributes()}>${depth < 2 ? inline(depth + 1) : word()}</${pick(['B', 'a.b', 'x:y'])}>`,
          () => `<${pick(['B', 'C'])}${attributes()} />`,
          () => `{${expression()}}`,
        )
      }
      parts.push(pick(choices)())
    }
    return parts.join(chance(0.7) ? ' ' : '')
  }

  const expression = () => pick(['a', '1 + 1', '"x"', "'}'", '`t${a}`', 'a.b', '[1, 2]', '{a: 1}', 'f(a, b)', '/* c */ a'])
  const attributes = () =>
    Array.from({length: Math.floor(next() * 3)}, () =>
      pick([` a="${word()}"`, ` b='${word()}'`, ` c={${expression()}}`, ' d', ` {...${pick(['p', 'q.r'])}}`, ` e = "${word()}"`]),
    ).join('')

  const block = (depth = 0) => {
    const choices = [
      () => inline(),
      () => `${'#'.repeat(1 + Math.floor(next() * 6))} ${inline()}`,
      () => `${inline()}\n${chance(0.5) ? '===' : '---'}`,
      () => pick(['***', '---', '___', '- - -']),
      () => `\`\`\`${pick(['', 'js', 'rs x'])}\n${words()}\n${chance(0.3) ? '\n' : ''}\`\`\``,
      () => `    ${words()}`,
      () => `> ${block(depth + 1).replaceAll('\n', '\n> ')}`,
      () => list(depth),
      () => `[r${Math.floor(next() * 2)}]: /${word()}${chance(0.3) ? ` "${word()}"` : ''}`,
    ]
    if (!mdx) {
      choices.push(
        () => `| ${word()} | ${word()} |\n|${pick([':-', '-', '-:', ':-:'])}|${pick(['-', ':-'])}|\n| ${word()} | ${word()} |`,
        () => `$$\n${word()}\n$$`,
        () => `[^f${Math.floor(next() * 2)}]: ${inline()}`,
        () => `<div>\n${word()}\n</div>`,
        () => `<!-- ${word()} -->`,
      )
    } else {
      choices.push(
        () => `<${pick(['A', 'B'])}${attributes()} />`,
        () => `{${expression()}}`,
        () => {
          const name = pick(['A', 'B', 'a.b'])
          return `<${name}${attributes()}>\n\n${depth < 2 ? block(depth + 1) : word()}\n\n</${name}>`
        },
        () => `<${pick(['A', 'B'])}>${inline()}</${pick(['A', 'B'])}>`,
      )
    }
    return pick(choices)()
  }

  const list = (depth) => {
    const ordered = chance(0.4)
    const items = Array.from({length: 1 + Math.floor(next() * 3)}, (_, i) => {
      const marker = ordered ? `${i + 1}.` : pick(['-', '*', '+'])
      const task = !ordered && !mdx && chance(0.2) ? pick(['[ ] ', '[x] ']) : ''
      const nested = depth < 2 && chance(0.3) ? `\n${list(depth + 1).replaceAll(/^/gm, '  ')}` : ''
      return `${marker} ${task}${inline()}${nested}`
    })
    return items.join(chance(0.2) ? '\n\n' : '\n')
  }

  const documents = []
  for (let i = 0; i < count; i++) {
    const blocks = Array.from({length: 1 + Math.floor(next() * 5)}, () => block())
    // A document that starts with a line of dashes makes the official parser read what follows as text.
    if (/^(---|- - -)/.test(blocks[0])) blocks[0] = '***'
    if (mdx && chance(0.15)) blocks.unshift(pick(['import a from "b"', 'export const x = 1']))
    if (!mdx && chance(0.1)) blocks.unshift(`---\ntitle: ${word()}\n---`)
    documents.push(blocks.join('\n\n') + '\n')
  }
  return documents
}
