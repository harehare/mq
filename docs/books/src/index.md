<div class="mq-hero">
  <div class="mq-hero-brand">
    <img src="./images/logo.svg" class="mq-hero-logo" alt="mq logo" />
    <h1 class="mq-hero-title">mq</h1>
  </div>
  <p class="mq-hero-tagline">Query. Filter. Transform <span class="mq-accent">Markdown.</span></p>
  <p class="mq-hero-desc">
    A command-line tool that processes Markdown using a syntax similar to jq.
    Written in Rust — slice, filter, map, and transform with ease.
  </p>
  <div class="mq-hero-actions">
    <a href="start/install.html" class="mq-btn-primary">Get Started</a>
    <a href="https://github.com/harehare/mq" class="mq-btn-secondary" target="_blank">GitHub</a>
    <a href="https://mqlang.org/playground" class="mq-btn-secondary" target="_blank">Playground</a>
  </div>
</div>

<div class="mq-terminal">
  <div class="mq-terminal-bar">
    <span class="mq-terminal-dot" style="background:#ef4444;opacity:.5"></span>
    <span class="mq-terminal-dot" style="background:#94a3b8;opacity:.5"></span>
    <span class="mq-terminal-dot" style="background:#67b8e3;opacity:.5"></span>
  </div>
  <div class="mq-terminal-body">
    <div>
      <span class="mq-terminal-prompt">$</span>
      <span class="mq-terminal-cmd"> cat README.md | mq </span><span class="mq-terminal-str">'.h2 | to_text()'</span>
    </div>
    <div style="margin-top:.5rem">
      <span class="mq-terminal-comment"># Output:</span><br/>
      <span class="mq-terminal-out">Getting Started</span><br/>
      <span class="mq-terminal-out">Features</span><br/>
      <span class="mq-terminal-out">Installation</span>
    </div>
  </div>
</div>

<p class="mq-section-title">Why mq?</p>

<div class="mq-card-grid">
  <div class="mq-card">
    <p class="mq-card-title">LLM Workflows</p>
    <p class="mq-card-desc">Efficiently manipulate and process Markdown used in LLM prompts and outputs.</p>
  </div>
  <div class="mq-card">
    <p class="mq-card-title">Documentation Management</p>
    <p class="mq-card-desc">Extract, transform, and organize content across multiple documentation files.</p>
  </div>
  <div class="mq-card">
    <p class="mq-card-title">Batch Processing</p>
    <p class="mq-card-desc">Apply consistent transformations across multiple Markdown files with sub-millisecond execution.</p>
  </div>
  <div class="mq-card">
    <p class="mq-card-title">Content Analysis</p>
    <p class="mq-card-desc">Quickly extract specific sections or patterns from Markdown documents with precise node selection.</p>
  </div>
</div>

<p class="mq-section-title">Features</p>

<div class="mq-feature-grid">
  <div class="mq-feature">
    <span class="mq-feature-text"><strong>Slice and Filter</strong> — Extract specific parts of your Markdown documents with ease.</span>
  </div>
  <div class="mq-feature">
    <span class="mq-feature-text"><strong>Map and Transform</strong> — Apply transformations to your Markdown content.</span>
  </div>
  <div class="mq-feature">
    <span class="mq-feature-text"><strong>CLI</strong> — Simple and intuitive command-line interface for quick operations.</span>
  </div>
  <div class="mq-feature">
    <span class="mq-feature-text"><strong>Extensibility</strong> — Easily extendable with custom functions and modules.</span>
  </div>
  <div class="mq-feature">
    <span class="mq-feature-text"><strong>Built-in Support</strong> — Rich set of built-in functions and selectors.</span>
  </div>
  <div class="mq-feature">
    <span class="mq-feature-text"><strong>REPL</strong> — Interactive command-line REPL for testing and experimenting.</span>
  </div>
  <div class="mq-feature">
    <span class="mq-feature-text"><strong>IDE Support</strong> — VSCode Extension and Language Server Protocol (LSP) support.</span>
  </div>
  <div class="mq-feature">
    <span class="mq-feature-text"><strong>Debugger</strong> — Inspect and step through mq queries interactively.</span>
  </div>
</div>
