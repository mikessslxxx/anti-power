import DOMPurify from 'dompurify';
import { Marked, type Tokens } from 'marked';

const escapeHtml = (text: string) =>
  text.replace(/[&<>"']/g, ch => `&#${ch.charCodeAt(0)};`);

// Parse a file:// URL into a local path and optional #L line
export function fileLink(href: string): { path: string; line: string | null } | null {
  if (!/^file:/i.test(href)) return null;
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return null;
  }
  let path = url.pathname;
  try {
    path = decodeURIComponent(path);
  } catch {
    // Keep the encoded form
  }
  const host = url.hostname && url.hostname.toLowerCase() !== 'localhost' ? url.hostname : '';
  // file://server/share/x is a Windows network share
  if (host) path = `\\\\${host}${path.replace(/\//g, '\\')}`;
  // file:///C:/x on Windows
  else if (/^\/[a-z]:\//i.test(path)) path = path.slice(1);
  const line = /^#L(\d+)/i.exec(url.hash)?.[1] ?? null;
  return path && path !== '/' ? { path, line } : null;
}

const fileIcon = '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 1.75h5L12.25 5v9.25H4z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/><path d="M9 1.75V5h3.25" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/></svg>';

const marked = new Marked({
  gfm: true,
  breaks: true,
  renderer: {
    // Show raw HTML as text so messages cannot inject markup
    html({ text }) {
      return escapeHtml(text);
    },
    // Never load remote images, only mention them
    image({ href, text }) {
      const label = escapeHtml(text || href);
      return /^https?:\/\//i.test(href)
        ? `<a href="${escapeHtml(href)}" class="md-image">${label}</a>`
        : `<span class="md-image">${label}</span>`;
    },
    link(this: { parser: { parseInline(tokens: Tokens.Generic[]): string } }, token: Tokens.Link) {
      const text = this.parser.parseInline(token.tokens);
      const file = fileLink(token.href);
      if (file) {
        const line = file.line ? `<span class="md-file-line">:${file.line}</span>` : '';
        return `<a href="#" class="md-file" data-path="${escapeHtml(file.path)}" title="${escapeHtml(file.path)}">${fileIcon}<span>${text}</span>${line}</a>`;
      }
      if (/^(https?:|mailto:)/i.test(token.href)) {
        return `<a href="${escapeHtml(token.href)}" title="${escapeHtml(token.href)}">${text}</a>`;
      }
      return text;
    },
    checkbox({ checked }) {
      return `<input type="checkbox" disabled${checked ? ' checked' : ''}> `;
    },
    code({ text, lang }) {
      const label = lang ? `<span class="md-code-lang">${escapeHtml(lang.split(/\s/)[0])}</span>` : '';
      return `<div class="md-code">${label}<pre><code>${escapeHtml(text)}</code></pre></div>`;
    },
  },
});

// Raw HTML is escaped above, so inputs only come from task lists; keep them inert anyway
DOMPurify.addHook('afterSanitizeAttributes', node => {
  if (node.nodeName === 'INPUT') {
    node.setAttribute('type', 'checkbox');
    node.setAttribute('disabled', '');
  }
});

export function renderMarkdown(source: string): string {
  const html = marked.parse(source, { async: false });
  return DOMPurify.sanitize(html, {
    ALLOWED_ATTR: ['href', 'title', 'class', 'data-path', 'viewBox', 'd', 'fill', 'stroke', 'stroke-width', 'stroke-linejoin', 'aria-hidden', 'start', 'align', 'type', 'checked', 'disabled'],
    FORBID_TAGS: ['style', 'img', 'iframe', 'form', 'button', 'select', 'textarea'],
  });
}
