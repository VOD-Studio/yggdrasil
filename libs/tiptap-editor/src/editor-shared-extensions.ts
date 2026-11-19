import { FileHandler } from '@tiptap/extension-file-handler';
import type { UploadCoordinator } from './upload-coordinator';

export const LINK_OPTIONS = {
  openOnClick: false,
  autolink: true,
  linkOnPaste: true,
  HTMLAttributes: { rel: 'noopener noreferrer', target: '_blank' },
};

export function imageFileHandler(getCoordinator: () => UploadCoordinator | null) {
  return FileHandler.configure({
    allowedMimeTypes: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'],
    onPaste: (_editor, files) => {
      files.forEach((file) => {
        getCoordinator()?.insertUploading(file);
      });
    },
    onDrop: (_editor, files, pos) => {
      files.forEach((file) => {
        getCoordinator()?.insertUploading(file, pos);
      });
    },
  });
}
