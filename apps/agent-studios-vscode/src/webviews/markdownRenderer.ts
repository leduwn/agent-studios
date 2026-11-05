/**
 * Agent Studios — Safe Markdown & HTML Escaping Utilities
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

/**
 * Escapes unsafe HTML characters to prevent XSS.
 */
export function escapeHtml(text: string | null | undefined): string {
  if (text === null || text === undefined) {
    return '';
  }
  return String(text)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/**
 * Renders safe, sanitized Markdown into HTML without external dependencies.
 * Guarantees zero script execution by escaping all input prior to markdown transformation.
 */
export function renderSafeMarkdown(markdown: string | null | undefined): string {
  if (!markdown || !markdown.trim()) {
    return '<p class="empty-markdown">No details provided.</p>';
  }

  // 1. Separate code blocks to avoid messing with formatting inside code
  const codeBlocks: string[] = [];
  const placeholder = (idx: number) => `@@CODEBLOCK_${idx}@@`;

  // Normalize line endings
  let text = markdown.replace(/\r\n/g, '\n').replace(/\r/g, '\n');

  // Extract fenced code blocks
  text = text.replace(/```([a-zA-Z0-9_-]*)\n([\s\S]*?)```/g, (_match, lang, code) => {
    const escapedCode = escapeHtml(code.trimEnd());
    const escapedLang = escapeHtml(lang.trim());
    const langAttr = escapedLang ? ` class="language-${escapedLang}"` : '';
    const block = `<pre class="code-block"><code${langAttr}>${escapedCode}</code></pre>`;
    const idx = codeBlocks.length;
    codeBlocks.push(block);
    return placeholder(idx);
  });

  // 2. Escape HTML for all remaining markdown text
  text = escapeHtml(text);

  // 3. Process line-based elements
  const lines = text.split('\n');
  const result: string[] = [];
  let inUl = false;
  let inOl = false;
  let inBlockquote = false;
  let blockquoteBuffer: string[] = [];

  const flushBlockquote = () => {
    if (inBlockquote) {
      result.push(`<blockquote>${blockquoteBuffer.join('<br />')}</blockquote>`);
      blockquoteBuffer = [];
      inBlockquote = false;
    }
  };

  const flushLists = () => {
    if (inUl) {
      result.push('</ul>');
      inUl = false;
    }
    if (inOl) {
      result.push('</ol>');
      inOl = false;
    }
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // Check for codeblock placeholder
    const cbMatch = trimmed.match(/^@@CODEBLOCK_(\d+)@@$/);
    if (cbMatch) {
      flushLists();
      flushBlockquote();
      const idx = parseInt(cbMatch[1], 10);
      result.push(codeBlocks[idx] || '');
      continue;
    }

    // Blank line
    if (!trimmed) {
      flushLists();
      flushBlockquote();
      continue;
    }

    // Horizontal rule
    if (/^---+$|^===+$|^\*\*\*+$/.test(trimmed)) {
      flushLists();
      flushBlockquote();
      result.push('<hr class="divider" />');
      continue;
    }

    // Blockquote (handles &gt; due to HTML escaping)
    const bqMatch = line.match(/^(?:&gt;|>)\s*(.*)$/);
    if (bqMatch) {
      flushLists();
      inBlockquote = true;
      blockquoteBuffer.push(formatInline(bqMatch[1]));
      continue;
    } else {
      flushBlockquote();
    }

    // Headings (escaped '#' characters will just be '#')
    const hMatch = line.match(/^(#{1,6})\s+(.*)$/);
    if (hMatch) {
      flushLists();
      const level = hMatch[1].length;
      result.push(`<h${level}>${formatInline(hMatch[2])}</h${level}>`);
      continue;
    }

    // Unordered lists
    const ulMatch = line.match(/^[-*+]\s+(.*)$/);
    if (ulMatch) {
      if (inOl) {
        result.push('</ol>');
        inOl = false;
      }
      if (!inUl) {
        result.push('<ul>');
        inUl = true;
      }
      result.push(`<li>${formatInline(ulMatch[1])}</li>`);
      continue;
    }

    // Ordered lists
    const olMatch = line.match(/^(\d+)\.\s+(.*)$/);
    if (olMatch) {
      if (inUl) {
        result.push('</ul>');
        inUl = false;
      }
      if (!inOl) {
        result.push('<ol>');
        inOl = true;
      }
      result.push(`<li>${formatInline(olMatch[2])}</li>`);
      continue;
    }

    // Standard paragraph
    flushLists();
    result.push(`<p>${formatInline(line)}</p>`);
  }

  flushLists();
  flushBlockquote();

  // Restore any codeblock placeholders that might have been inline or missed
  let finalHtml = result.join('\n');
  for (let idx = 0; idx < codeBlocks.length; idx++) {
    finalHtml = finalHtml.replace(new RegExp(`@@CODEBLOCK_${idx}@@`, 'g'), codeBlocks[idx]);
  }

  return finalHtml;
}

/**
 * Formats inline Markdown elements (code, bold, italic, links).
 */
function formatInline(text: string): string {
  // Inline code: `code`
  let res = text.replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>');

  // Bold: **text** or __text__
  res = res.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  res = res.replace(/__([^_]+)__/g, '<strong>$1</strong>');

  // Italic: *text* or _text_
  res = res.replace(/\*([^*]+)\*/g, '<em>$1</em>');
  res = res.replace(/_([^_]+)_/g, '<em>$1</em>');

  // Safe links: [text](https://url)
  res = res.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)"]+)\)/g, (_m, label, url) => {
    return `<a href="${url}" target="_blank" rel="noopener noreferrer" class="external-link">${label}</a>`;
  });

  return res;
}
