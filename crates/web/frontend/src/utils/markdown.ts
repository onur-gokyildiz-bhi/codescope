// Minimal markdown → HTML renderer. Purpose: render archive content without
// pulling an external dep. Handles: headers, bold, italic, inline code, fenced
// code blocks, unordered lists, ordered lists, blockquotes, links, hr, and
// paragraphs. Preserves whitespace in code blocks.
//
// Security: we escape HTML in all non-code paths, so arbitrary JSONL content
// can't inject script tags. Code blocks escape content but keep it in <pre>.

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function renderInline(s: string): string {
  let out = escapeHtml(s);
  // inline code first (greedy-safe via non-backtick class)
  out = out.replace(/`([^`\n]+)`/g, '<code>$1</code>');
  // bold before italic
  out = out.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>');
  out = out.replace(/\*([^*\n]+)\*/g, '<em>$1</em>');
  out = out.replace(/_([^_\n]+)_/g, '<em>$1</em>');
  // links: [text](url) — url must not contain spaces
  out = out.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
  return out;
}

export function renderMarkdown(src: string): string {
  const lines = (src || '').split('\n');
  const out: string[] = [];
  let i = 0;

  const flushParagraph = (buf: string[]) => {
    if (buf.length === 0) return;
    const text = buf.join(' ').trim();
    if (text) out.push(`<p>${renderInline(text)}</p>`);
    buf.length = 0;
  };

  let paragraph: string[] = [];

  while (i < lines.length) {
    const line = lines[i];

    // fenced code block
    const fence = line.match(/^```(\w*)\s*$/);
    if (fence) {
      flushParagraph(paragraph);
      const lang = fence[1] || '';
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        body.push(lines[i]);
        i++;
      }
      i++; // skip closing fence
      out.push(`<pre class="md-code${lang ? ` lang-${lang}` : ''}"><code>${escapeHtml(body.join('\n'))}</code></pre>`);
      continue;
    }

    // header
    const hdr = line.match(/^(#{1,6})\s+(.+?)\s*$/);
    if (hdr) {
      flushParagraph(paragraph);
      const level = hdr[1].length;
      out.push(`<h${level} class="md-h${level}">${renderInline(hdr[2])}</h${level}>`);
      i++;
      continue;
    }

    // hr
    if (/^[-*_]{3,}\s*$/.test(line)) {
      flushParagraph(paragraph);
      out.push('<hr class="md-hr" />');
      i++;
      continue;
    }

    // unordered list
    if (/^\s*[-*+]\s+/.test(line)) {
      flushParagraph(paragraph);
      const items: string[] = [];
      while (i < lines.length && /^\s*[-*+]\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*[-*+]\s+/, ''));
        i++;
      }
      out.push('<ul class="md-ul">' + items.map((it) => `<li>${renderInline(it)}</li>`).join('') + '</ul>');
      continue;
    }

    // ordered list
    if (/^\s*\d+\.\s+/.test(line)) {
      flushParagraph(paragraph);
      const items: string[] = [];
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*\d+\.\s+/, ''));
        i++;
      }
      out.push('<ol class="md-ol">' + items.map((it) => `<li>${renderInline(it)}</li>`).join('') + '</ol>');
      continue;
    }

    // blockquote
    if (/^>\s?/.test(line)) {
      flushParagraph(paragraph);
      const quoted: string[] = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) {
        quoted.push(lines[i].replace(/^>\s?/, ''));
        i++;
      }
      out.push(`<blockquote class="md-bq">${renderInline(quoted.join(' '))}</blockquote>`);
      continue;
    }

    // blank line ends paragraph
    if (line.trim() === '') {
      flushParagraph(paragraph);
      i++;
      continue;
    }

    paragraph.push(line);
    i++;
  }

  flushParagraph(paragraph);
  return out.join('\n');
}
