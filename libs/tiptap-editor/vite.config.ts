import { iifeLib } from '../vite-iife.ts';

export default iifeLib({
  dir: import.meta.dirname,
  out: 'tiptap',
  name: 'TiptapEditor',
  file: 'editor',
  defaultExport: true,
});
