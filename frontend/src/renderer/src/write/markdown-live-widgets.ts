// CM6 custom widgets for live markdown preview
// Renders images and other block elements inline within the editor

import { Decoration, WidgetType, EditorView } from '@codemirror/view';

/**
 * Inline image widget — renders an <img> element inside the CodeMirror editor.
 * Clicking on the image focuses back to the source markdown.
 */
export class ImageWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly alt: string,
  ) {
    super();
  }

  eq(other: ImageWidget): boolean {
    return other.src === this.src && other.alt === this.alt;
  }

  toDOM(): HTMLElement {
    const container = document.createElement('span');
    container.className = 'cm-live-image';
    container.style.display = 'block';
    container.style.margin = '12px 0';
    container.style.cursor = 'pointer';

    const img = document.createElement('img');
    img.src = this.src;
    img.alt = this.alt;
    img.style.maxWidth = '100%';
    img.style.borderRadius = '6px';
    img.style.display = 'block';
    container.appendChild(img);

    return container;
  }

  ignoreEvent(): boolean {
    return false; // Allow click events to pass through
  }
}

/**
 * Find image syntax `![alt](src)` in a line and return Decoration replacements.
 * Each image is replaced with an ImageWidget that renders the actual image inline.
 */
export function buildImageDecorations(
  lineText: string,
  lineFrom: number,
  _view: EditorView,
): Decoration[] {
  const decorations: Decoration[] = [];
  const regex = /!\[([^\]]*)\]\(([^)]+)\)/g;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(lineText)) !== null) {
    const start = lineFrom + match.index;
    const end = start + match[0].length;
    const alt = match[1];
    const src = match[2];

    decorations.push(
      Decoration.replace({
        widget: new ImageWidget(src, alt),
        inclusive: false,
      }).range(start, end),
    );
  }

  return decorations;
}
