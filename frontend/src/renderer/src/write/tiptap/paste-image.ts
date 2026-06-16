// Image paste/drop handler for TipTap editor
// Intercepts clipboard/drag images and inserts them as base64 data URLs

import type { Editor } from '@tiptap/react';

let imageCounter = 0;

function nextImageName(ext: string): string {
  imageCounter += 1;
  return `image-${imageCounter}.${ext}`;
}

/**
 * Handle image paste events: read image blobs from clipboard,
 * convert to data URLs, and insert into the TipTap editor.
 */
export async function handleImagePaste(
  editor: Editor,
  clipboardData: DataTransfer,
  _workspaceRoot: string
): Promise<boolean> {
  const items = clipboardData.items;
  let handled = false;

  for (let idx = 0; idx < items.length; idx++) {
    const item = items[idx];
    if (!item.type.startsWith('image/')) continue;

    const blob = item.getAsFile();
    if (!blob) continue;

    const ext = item.type.split('/')[1] || 'png';
    const name = nextImageName(ext);

    const dataUrl = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });

    editor
      .chain()
      .focus()
      .setImage({ src: dataUrl, alt: name })
      .run();

    handled = true;
  }

  return handled;
}

/**
 * Handle image drop events: read dropped image files,
 * convert to data URLs, and insert into the TipTap editor.
 */
export async function handleImageDrop(
  editor: Editor,
  event: DragEvent,
  _workspaceRoot: string
): Promise<boolean> {
  const files = event.dataTransfer?.files;
  if (!files || files.length === 0) return false;

  let handled = false;

  for (let idx = 0; idx < files.length; idx++) {
    const file = files[idx];
    if (!file.type.startsWith('image/')) continue;

    const ext = file.name.split('.').pop() || 'png';
    const name = nextImageName(ext);

    const dataUrl = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });

    editor
      .chain()
      .focus()
      .setImage({ src: dataUrl, alt: name })
      .run();

    handled = true;
  }

  return handled;
}
