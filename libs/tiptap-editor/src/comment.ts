import { buildCommentExtensions, commentEditorProps } from './comment-extensions';
import { type CommonEditorOptions, EditorInstance, editorLibrary } from './editor-instance';
import './style.css';
import './code-highlight.css';

class CommentEditor extends EditorInstance {
  constructor(container: HTMLElement, options: CommonEditorOptions = {}, onDestroy?: () => void) {
    super(container, options, onDestroy);
    const element = document.createElement('div');
    element.className = 'tiptap-editor tiptap-editor--comment';
    container.appendChild(element);
    this.mountEditor(element, {
      extensions: buildCommentExtensions(this.getCoordinator, options.placeholder),
      editorProps: commentEditorProps,
    });
    options.onReady?.();
  }
}

export default editorLibrary(CommentEditor);
