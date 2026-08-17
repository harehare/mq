import { StreamLanguage } from "@codemirror/language";
import type { StringStream } from "@codemirror/language";

type MqState = {
  inString: boolean;
};

const KEYWORDS =
  /^\b(let|def|do|match|while|until|unless|foreach|if|elif|else|end|self|None|nodes|break|continue|include|import|module|var|loop)\b/;
const OPERATORS =
  /^(\/\/=|<<|>>|\|\||\?\?|<=|>=|==|!=|=~|&&|\+=|-=|\*=|\/=|\|=|=|\||:|;|\?|!|\+|-|\*|\/|%|<|>|@)/;
const FUNCTION_CALL = /^[a-zA-Z_]\w*(?=\s*\()/;
const IDENTIFIER = /^[a-zA-Z_]\w*/;

function token(stream: StringStream, state: MqState): string | null {
  if (state.inString) {
    if (stream.match(/^\$\{[^}]*\}/)) return "variableName";
    if (stream.match(/^\\./)) return "escape";
    if (stream.eat('"')) {
      state.inString = false;
      return "string";
    }
    if (stream.match(/^[^\\"]+/)) return "string";
    stream.next();
    return "string";
  }

  if (stream.eatSpace()) return null;

  if (stream.match(/^#[^\n]*/)) return "comment";
  if (stream.match(KEYWORDS)) return "keyword";
  if (stream.match(OPERATORS)) return "operator";
  if (stream.match('"')) {
    state.inString = true;
    return "string";
  }
  if (stream.match(/^\d+/)) return "number";
  if (stream.match(FUNCTION_CALL)) return "variableName.function";
  if (stream.match(/^[()[\]]/)) return "paren";
  if (stream.match(IDENTIFIER)) return "variableName";

  stream.next();
  return null;
}

export const mqLanguage = StreamLanguage.define<MqState>({
  name: "mq",
  startState: () => ({ inString: false }),
  token,
  languageData: {
    commentTokens: { line: "#" },
    closeBrackets: { brackets: ["(", "[", '"'] },
  },
});
