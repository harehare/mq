/*
 * highlight.js syntax definition for mq language
 * mq is a jq-like tool for markdown processing
 */
(function () {
  function hljsDefineMq(hljs) {
    const KEYWORDS = {
      keyword:
        "def do let if elif else end while foreach self nodes match fn break continue include import module var macro quote unquote loop try catch",
      literal: "true false None",
    };

    const COMMENT = hljs.COMMENT("#", "$");

    const NUMBER = {
      className: "number",
      begin: "\\b[0-9]+(?:\\.[0-9]+)?\\b",
      relevance: 0,
    };

    const STRING = {
      className: "string",
      begin: '"',
      end: '"',
      contains: [
        {
          className: "char.escape",
          begin: "\\\\.",
        },
      ],
    };

    const INTERPOLATED_STRING = {
      className: "string",
      begin: 's"',
      end: '"',
      contains: [
        {
          className: "char.escape",
          begin: "\\\\.",
        },
        {
          className: "subst",
          begin: "\\$\\{",
          end: "\\}",
          contains: [
            {
              className: "variable",
              begin: "[a-zA-Z_][a-zA-Z0-9_]*",
            },
          ],
        },
      ],
    };

    const SYMBOL = {
      className: "symbol",
      begin: ":[a-zA-Z_][a-zA-Z0-9_]*",
    };

    const SELECTOR = {
      className: "title.function",
      begin: "\\.[a-zA-Z_\\[][a-zA-Z0-9_\\[\\]]*",
    };

    const FUNCTION_DEF = {
      className: "function",
      beginKeywords: "def fn",
      end: "\\(",
      excludeEnd: true,
      contains: [
        {
          className: "title.function",
          begin: "[a-zA-Z_][a-zA-Z0-9_]*",
        },
      ],
    };

    const FUNCTION_CALL = {
      className: "title.function.invoke",
      begin: "\\b[a-zA-Z_][a-zA-Z0-9_]*\\s*\\(",
      returnBegin: true,
      contains: [
        {
          className: "title.function",
          begin: "[a-zA-Z_][a-zA-Z0-9_]*",
        },
      ],
    };

    const VARIABLE = {
      className: "variable",
      begin: "\\$[a-zA-Z_][a-zA-Z0-9_]*",
    };

    return {
      name: "mq",
      aliases: ["mq"],
      keywords: KEYWORDS,
      contains: [
        COMMENT,
        INTERPOLATED_STRING,
        STRING,
        NUMBER,
        SYMBOL,
        SELECTOR,
        FUNCTION_DEF,
        FUNCTION_CALL,
        VARIABLE,
      ],
    };
  }

  // Register the language with highlight.js
  if (typeof hljs !== "undefined") {
    hljs.registerLanguage("mq", hljsDefineMq);

    // Re-highlight all mq code blocks after registration
    // Need to reset the content since book.js already processed them
    document.querySelectorAll("pre code.language-mq").forEach((block) => {
      // Remove hljs class to allow re-highlighting
      block.classList.remove("hljs");
      // Reset content to plain text (remove any existing highlighting spans)
      block.textContent = block.textContent;
      // Re-apply highlighting with mq language
      hljs.highlightBlock(block);
    });
  }
})();
