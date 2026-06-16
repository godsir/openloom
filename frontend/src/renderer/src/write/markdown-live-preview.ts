// CM6 ViewPlugin — live Markdown decorations
// Hides Markdown syntax markers (*, #, ~~, etc.) for non-active lines

import {
  Decoration,
  DecorationSet,
  EditorView,
  ViewPlugin,
  ViewUpdate,
  WidgetType,
} from '@codemirror/view';
import { RangeSetBuilder } from '@codemirror/state';

/** Zero-size widget used to hide markdown markers */
class ZeroWidthWidget extends WidgetType {
  toDOM(): HTMLElement {
    const span = document.createElement('span');
    span.style.display = 'none';
    span.setAttribute('aria-hidden', 'true');
    return span;
  }

  eq(_other: ZeroWidthWidget): boolean {
    return true;
  }
}

const zeroWidth = new ZeroWidthWidget();

/**
 * Font size mapping for heading levels.
 * Level 1 = 1.8em, Level 2 = 1.5em, Level 3 = 1.3em,
 * Level 4 = 1.1em, Level 5 = 1em, Level 6 = 0.9em
 */
const HEADING_FONT_SIZES: Record<number, string> = {
  1: '1.8em',
  2: '1.5em',
  3: '1.3em',
  4: '1.1em',
  5: '1em',
  6: '0.9em',
};

function buildLiveDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const { doc } = view.state;
  const cursorPos = view.state.selection.main.head;

  for (let i = 1; i <= doc.lines; i++) {
    const line = doc.line(i);
    const text = line.text;
    const lineFrom = line.from;
    const lineTo = line.to;

    // Check if cursor is on this line
    const cursorOnLine = cursorPos >= lineFrom && cursorPos <= lineTo;

    // --- Heading: hide # prefix, apply font styling ---
    const headingMatch = text.match(/^(#{1,6})\s+(.*)/);
    if (headingMatch) {
      const hashLen = headingMatch[1].length + 1; // +1 for the space
      // Hide the # prefix
      builder.add(
        lineFrom,
        lineFrom + hashLen,
        Decoration.replace({ widget: zeroWidth }),
      );
      // Apply heading font size and weight
      const fontSize = HEADING_FONT_SIZES[hashLen - 1] || '1em';
      builder.add(
        lineFrom + hashLen,
        lineTo,
        Decoration.mark({
          attributes: {
            style: `font-size: ${fontSize}; font-weight: 600; display: inline-block;`,
          },
        }),
      );
      continue;
    }

    // --- Inline markers: hide only when cursor is NOT on this line ---
    if (!cursorOnLine) {
      hideMarkers(builder, text, lineFrom);
    }
  }

  return builder.finish();
}

function hideMarkers(
  builder: RangeSetBuilder<Decoration>,
  text: string,
  lineFrom: number,
): void {
  // Bold: **...**
  hidePattern(builder, text, lineFrom, /\*\*(.+?)\*\*/g, 2);

  // Strikethrough: ~~...~~
  hidePattern(builder, text, lineFrom, /~~(.+?)~~/g, 2);

  // Italic: *...* (but not **, handle single *)
  // Use a simpler approach: find *italic* but avoid **
  const italicRegex = /(?<!\*)\*(?!\*)(.+?)(?<!\*)\*(?!\*)/g;
  hidePattern(builder, text, lineFrom, italicRegex, 1);

  // Inline code: `...`
  hidePattern(builder, text, lineFrom, /`(.+?)`/g, 1);
}

function hidePattern(
  builder: RangeSetBuilder<Decoration>,
  text: string,
  lineFrom: number,
  regex: RegExp,
  markerLen: number,
): void {
  let match: RegExpExecArray | null;
  regex.lastIndex = 0;
  while ((match = regex.exec(text)) !== null) {
    const start = lineFrom + match.index;
    const end = start + match[0].length;

    // Hide opening marker
    builder.add(
      start,
      start + markerLen,
      Decoration.replace({ widget: zeroWidth }),
    );
    // Hide closing marker
    builder.add(
      end - markerLen,
      end,
      Decoration.replace({ widget: zeroWidth }),
    );
  }
}

/**
 * Create the live preview ViewPlugin for CodeMirror 6.
 * Rebuilds decorations when document, viewport, or selection changes.
 */
export function createLivePreviewPlugin() {
  return ViewPlugin.fromClass(
    class LivePreviewPlugin {
      decorations: DecorationSet;

      constructor(view: EditorView) {
        this.decorations = buildLiveDecorations(view);
      }

      update(update: ViewUpdate) {
        if (
          update.docChanged ||
          update.viewportChanged ||
          update.selectionSet
        ) {
          this.decorations = buildLiveDecorations(update.view);
        }
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );
}
