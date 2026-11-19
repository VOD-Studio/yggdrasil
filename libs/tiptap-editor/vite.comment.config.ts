import { iifeLib } from '../vite-iife.ts';

export default iifeLib({
  dir: import.meta.dirname,
  out: 'tiptap',
  name: 'TiptapCommentEditor',
  file: 'comment',
  entry: 'src/comment.ts',
  defaultExport: true,
  emptyOutDir: false,
  define: { __YGG_TIPTAP_ASSETS__: 'true' },
});
