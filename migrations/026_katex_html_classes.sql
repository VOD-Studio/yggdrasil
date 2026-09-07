-- KaTeX 0.18 prefixes internal layout classes. The migration runner uses
-- lol_html to rewrite class tokens only inside rendered KaTeX HTML, not SQL
-- text replacements (which would also corrupt prose, code and attributes).
-- Hold writers until the HTML rewrite and version marker commit together.
-- Stop old application instances before deploying the new renderer/assets;
-- otherwise they could persist old markup again after this transaction.
LOCK TABLE posts, comments IN SHARE ROW EXCLUSIVE MODE;
