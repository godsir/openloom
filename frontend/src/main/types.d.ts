// type declarations for modules without built-in types
declare module 'html-to-docx' {
  export default function htmlToDocx(
    html: string,
    header?: unknown,
    options?: { title?: string; margins?: { top: number; bottom: number; left: number; right: number } },
  ): Promise<Buffer>;
}
