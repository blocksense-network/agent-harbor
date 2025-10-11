# Implementing Keyboard Shortcuts in tui-textarea and tui-input

## Built-in Shortcut Support in tui-textarea and tui-input

**tui-textarea** (for multi-line text) already provides many Emacs-style and common shortcuts out of the box. Its default key mappings cover most basic **cursor movements** and **editing operations**:

* **Cursor movement & navigation:** You can move the cursor by characters, words, lines, paragraphs, or to line/document boundaries using standard Emacs or CUA keys. For example, Ctrl+F/← moves forward one char, Ctrl+B/→ moves back one char, Ctrl+P/↑ and Ctrl+N/↓ move up/down lines, and Alt+F/Ctrl+→ and Alt+B/Ctrl+← jump by words[\[1\]](https://github.com/rhysd/tui-textarea#:~:text=,to%20the%20end%20of%20line). Likewise, Ctrl+A/Home jumps to line start, Ctrl+E/End to line end, Alt+\</Ctrl+Home to top of the document, and Alt+\>/Ctrl+End to bottom[\[2\]](https://github.com/rhysd/tui-textarea#:~:text=,Scroll%20up%20by%20page). Page navigation is also built-in (Ctrl+V/PageDown and Alt+V/PageUp scroll a page)[\[2\]](https://github.com/rhysd/tui-textarea#:~:text=,Scroll%20up%20by%20page). These default mappings mean you typically **don’t need to manually map** those keys – calling TextArea::input(event) on the incoming key events will handle them.

* **Editing, deletion & clipboard:** Standard deletion keys and text edits are handled internally. For example, Ctrl+H or Backspace delete the character before the cursor, Ctrl+D or Delete delete the next character[\[3\]](https://github.com/rhysd/tui-textarea#:~:text=Mappings%20Description%20,one%20word%20next%20to%20cursor). Word deletion is supported with Emacs Meta bindings and typical Ctrl keys (Alt+D/Ctrl+Delete delete word forward, Alt+Backspace/Ctrl+Backspace delete word backward)[\[3\]](https://github.com/rhysd/tui-textarea#:~:text=Mappings%20Description%20,one%20word%20next%20to%20cursor). tui-textarea also implements “kill line” and related commands: Ctrl+K kills (deletes) to end-of-line, and Ctrl+J kills to beginning-of-line[\[3\]](https://github.com/rhysd/tui-textarea#:~:text=Mappings%20Description%20,one%20word%20next%20to%20cursor). It even has an *undo/redo stack* and clipboard integration: by default Ctrl+U triggers Undo and Ctrl+R Redo, while Cut/Copy/Paste are mapped to Ctrl+X, Ctrl+C, Ctrl+Y respectively (mimicking CUA/Mac keys; Emacs Yank corresponds to Ctrl+Y)[\[4\]](https://github.com/rhysd/tui-textarea#:~:text=,Paste%20yanked%20text). Text that you cut or kill is stored in an internal yank buffer so it can be yanked/pasted later[\[4\]](https://github.com/rhysd/tui-textarea#:~:text=,Paste%20yanked%20text). In short, most common text editing shortcuts (character/word delete, cut/copy/paste, undo/redo, newline insertion with Enter, etc.) work out-of-the-box with tui-textarea’s input() handling.

**tui-input** (for single-line inputs) supports a similar subset of these shortcuts via its InputRequest API. The crate maps common key events to input requests under the hood. For instance, in tui-input pressing Backspace or Ctrl+H yields a DeletePrevChar request, and Delete yields DeleteNextChar[\[5\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L26-L34). Arrow keys and Emacs combos are handled too: Left or Ctrl+B move the cursor left (GoToPrevChar), Right or Ctrl+F move right (GoToNextChar), and using Ctrl+Left or Alt+B moves one word left (GoToPrevWord), while Ctrl+Right or Alt+F move one word right (GoToNextWord)[\[6\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L32-L40)[\[7\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L35-L42). Likewise, Ctrl+A or Home will jump to the start of the line (GoToStart), and Ctrl+E or End goes to line end (GoToEnd)[\[8\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L52-L57). On the editing side, tui-input uses Emacs-style kills for the single line: Ctrl+U clears the entire line (DeleteLine), Ctrl+W or Alt+Backspace deletes the previous word (DeletePrevWord), Alt+D/Ctrl+Delete deletes the next word (DeleteNextWord), and Ctrl+K kills from cursor to line end (DeleteTillEnd)[\[9\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L44-L53). In short, you can feed crossterm key events into Input::handle\_event() or convert them to InputRequest and Input::handle() – the library will perform the expected cursor moves and deletions internally. (Note: By default, Tab is ignored in tui-input[\[10\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L30-L38), since it’s usually used for focus navigation in single-line inputs.)

**Placeholders:** Both libraries support placeholder text for inputs. For example, tui-textarea can display a gray hint when empty (see the “popup\_placeholder” example in its docs[\[11\]](https://github.com/rhysd/tui-textarea#:~:text=popup_placeholder)). You just need to configure the placeholder on the widget (e.g. via a method or during initialization). Ensure each input field’s placeholder is set to guide the user on expected content.

## Handling Additional Shortcuts (Custom Implementation Needed)

The large set of shortcuts listed includes many that are **not directly supported by the default APIs**. However, tui-textarea provides methods to manipulate text and cursor position programmatically, which you can use to implement these extra features. The general strategy is to **intercept the key events** for those operations and call the appropriate TextArea/Input methods (or perform custom logic) yourself. Below we discuss each category of shortcuts not covered by defaults, and how to implement them using the existing APIs.

### Extended Cursor Movements (Sentences, Paragraphs, Go-To, Recenter, Matching Parens)

* **Sentence navigation (M-a, M-e):** There is no built-in concept of “sentence” boundaries in tui-textarea. You’ll need to implement these by scanning the text around the cursor. For Meta+A (move to sentence beginning), scan backwards from the current cursor for the end of the previous sentence (e.g. a period . or \! or ? followed by a space or newline) and move the cursor there. Likewise, Meta+E should scan forward for the next sentence terminator. Once you determine the target position (row and column), call textarea.move\_cursor(CursorMove::Jump(row, col)) to jump there[\[12\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search). You might also leverage word-move as a helper (e.g., step word-by-word or use regex to find sentence patterns).

* **Move to matching parenthesis (Ctrl+Meta+F/B):** This isn’t built-in, but you can implement a simple parentheses matcher. On Ctrl+Meta+F, if the cursor is at or just before an opening bracket ((, {, \[), scan forward in the text buffer and use a counter to find its matching closing bracket, skipping nested pairs. If found, jump the cursor to that position. For Ctrl+Meta+B (backward), do the inverse for a closing bracket by scanning backward for its matching opener. This requires parsing the text – you can get the text via textarea.lines() (which gives you a slice of strings) and then locate the indices. Use CursorMove::Jump to set the cursor once the index is known. There’s no direct API for this, but it’s achievable with a custom function.

* **Go to line number (e.g. Emacs M-g g / Ctrl+G):** tui-textarea doesn’t have a native “goto line” dialog, so you’ll implement it manually. A common approach is to open a small prompt (perhaps using a tui-input field) to let the user enter a line number. Once you have the target line, use the jump API: textarea.move\_cursor(CursorMove::Jump(line\_index, 0)) to go to the start of that line[\[12\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search). (Remember to subtract 1 if the user enters a 1-based line number; TextArea uses 0-based indices for rows.) If the line is beyond the document end, you can clamp to the last line.

* **Recenter view (Ctrl+L):** By default the textarea automatically scrolls as needed to keep the cursor visible, but the specific “recenter cursor in middle of screen” behavior isn’t automatic. You can achieve this by adjusting the scroll position. tui-textarea provides a textarea.scroll((row, col)) method that scrolls the viewport to put a given text position at the top-left[\[13\]](https://github.com/rhysd/tui-textarea#:~:text=,page). To recenter, determine the current cursor’s line index and the height of the visible area (lines per page). Then calculate a target top line such that the cursor line ends up in the middle of the screen. For example, let top \= cursor\_line\_index.saturating\_sub(view\_height/2). Then call textarea.scroll((top, 0))[\[13\]](https://github.com/rhysd/tui-textarea#:~:text=,page). This will adjust the viewport so that the cursor is roughly centered. (There is no one-call “recenter”, so you handle it via the scrolling API as shown.)

### Text Transformation Shortcuts (Upper/Lower/Capitalization, Fill, Join Lines)

These Emacs shortcuts (M-u, M-l, M-c, M-q, M-^) are not built into the widgets, but you can implement them by combining existing edit operations:

* **Uppercase/Lowercase/Capitalize word:** To implement Meta+U (uppercase word), you need to take the word at or after the cursor and transform its case. One approach: use the word-movement commands to find the word boundaries. For example, in tui-textarea you could call start\_selection, then move\_cursor(WordForward) to select the next word, then retrieve the selected text. However, tui-textarea doesn’t directly return the selected substring via an API. Instead, you might use the yank mechanism: for instance, call textarea.copy() (Ctrl+C mapping) to copy the selection to the copy buffer (or cut and immediately undo). If the API doesn’t expose the clipboard content, an easier approach is to get the full text (or current line) via textarea.lines() and extract the substring by your own logic (using the known selection indices or cursor indices). Once you have the word, transform it (to upper-case, lower-case, etc. using Rust’s to\_uppercase() etc.), then replace it in the text area. You can replace it by deleting the original word and inserting the new text character by character using textarea.insert\_char(c) for each character. For example, to uppercase: mark the word boundaries, delete that word (e.g. using delete\_next\_word which will remove it and also save it in yank buffer)[\[3\]](https://github.com/rhysd/tui-textarea#:~:text=Mappings%20Description%20,one%20word%20next%20to%20cursor), then for each char in the uppercased word call textarea.insert\_char. (This will also be recorded in history for undo.) Implement M-l (lowercase) and M-c (capitalize) similarly, just altering the case conversion logic.

* **Fill paragraph (Meta+Q):** This command is essentially a re-wrap or justify operation for a paragraph of text. You’ll need to manually implement text reflow, since tui-textarea won’t reformat paragraphs on its own. One approach: identify the current paragraph boundaries – e.g. from the current cursor line, find the nearest blank line above and below to get the paragraph range. Extract those lines (using textarea.lines()\[start..end\] slice). Concatenate them into one long string, then insert line breaks at appropriate lengths (for example, wrap at 80 columns or the text area’s width) to achieve justification or filling. Finally, replace the original lines in the TextArea with the new wrapped lines. You can do this by deleting the paragraph’s lines and inserting the new ones. Since there’s no multi-line replace API, you might delete line-by-line: move cursor to start of the first line of the paragraph and call delete\_line\_by\_end repeatedly, or use delete\_next\_char at the end of each line to merge it with the next until the paragraph becomes one line, then insert new breaks. This is fairly involved: essentially you are acting as a text reflow algorithm. Another option is to perform the reflow outside (with a wrapping algorithm) and then reconstruct the TextArea from the new text for that paragraph (though reconstructing will reset undo history for those lines). In summary, M-q requires custom logic to gather and rewrite text, using the deletion and insertion methods provided by TextArea.

* **Join lines (Meta+^):** This is easier: joining two lines can be done by simply deleting the newline between them. In tui-textarea, if you place the cursor at the end of a line and invoke a forward delete, it will remove the newline and pull the next line up. For example, you can handle M-^ by checking that the cursor is not already at the last line, then calling textarea.delete\_next\_char() when at end-of-line (or equivalently, Delete key event at EOL) to merge the next line into the current one[\[14\]](https://github.com/rhysd/tui-textarea#:~:text=Method%20Operation%20,character%20before%20cursor). You could also simulate this by moving the cursor to the start of the next line and using delete\_prev\_char. The TextArea internally handles newline characters, so deleting one will concatenate the lines. If you want to join a block of multiple lines, you can loop calling the same operation for each intervening newline. Remember to possibly insert a space if needed (Emacs M-^ typically joins with a space if not already appropriately spaced).

### Markdown Formatting Shortcuts (Bold, Italic, Underline, Hyperlink)

These shortcuts (Ctrl+B/I/U/K) are not text-editor primitives but rather editing conveniences for rich text (Markdown). You’ll implement them at the application level using insertion and selection:

* **Bold (Ctrl+B)** / **Italic (Ctrl+I)** / **Underline (Ctrl+U):** The expected behavior is to wrap the selected text (or the next typed text) with markdown syntax (\*\* for bold, \* or \_ for italic, etc.). Since tui-textarea is a plain text editor, it won’t apply these by itself, but you can intercept the key. If the user has a selection, you can wrap the selection: for example, for **Bold**, surround the selected text with \*\* by inserting \*\* before and after it. You can do this by getting the selection range, inserting the prefix and suffix strings at those positions. If no text is selected, a common approach is to insert the markup and position the cursor appropriately. For instance, on Ctrl+B with no selection: insert \*\*\*\* at the cursor (which results in \*\*|\*\* with | being cursor), then move the cursor two characters back (so it lands between the two pairs of asterisks). To implement that: you could call textarea.insert\_str("\*\*\*\*") if such existed (it doesn’t, so insert each '\*' char four times), then call textarea.move\_cursor(CursorMove::Back) twice to reposition. Similar logic for italic (\* or \_ pair) and underline. (Underline isn’t standard Markdown, but if you want to support it as underlining selected text in output, you might wrap with \_\_ or use HTML tags via Markdown.) These operations don’t have dedicated crate functions, but they can be done with sequences of insert\_char and move\_cursor. It’s wise to integrate this with selection: if text was selected, you should insert the formatting around it (which means the selection will be replaced by the formatted version; you might need to retrieve the text, add markers, and replace).

* **Insert hyperlink (Ctrl+K):** Typically this would prompt the user for a URL and optionally link text, then insert a Markdown link \[text\](url). Implementation can be in two steps: on Ctrl+K, if some text is selected, treat that as the link text; if none selected, you might ask for link text as well. Prompt the user for the URL (perhaps using a tui-input field in a popup). Once you have the URL (and text), insert the string in the format \[link text\](<http://example.com>) at the cursor (replacing any selected text). Again, insertion is done via multiple insert\_char calls for each character of the brackets, text, parentheses, etc. You’ll likely want to place the cursor either at the end of the inserted link or at a convenient spot (maybe between the brackets if no link text was provided so the user can type it). All of this logic lives in your event handler when catching the Ctrl+K key – tui-textarea won’t do it for you.

### Code Editing Shortcuts (Comment, Duplicate, Move Lines, Indent/Dedent)

These kinds of editor operations go beyond basic text insertion/deletion, but you can achieve them with the API:

* **Toggle comment (Meta+; or Ctrl+/):** This operation adds or removes a comment marker (like // or \#) at the start of line(s). To implement it, decide on the comment syntax based on context (perhaps your app knows if it’s editing code). When the shortcut is pressed, check if there is a text selection spanning multiple lines or just a single line (cursor). If multiple lines are selected, you’ll want to comment/uncomment all of them. You can iterate through each affected line: move cursor to the beginning of the line (CursorMove::Head) and insert or remove the comment token. For inserting, just call insert\_char for each character of the token (e.g. '/','/'). For removing, check if the line starts with that token: you can read the line (from textarea.lines()\[i\]) or move cursor to start and call delete\_next\_char() twice to remove //. This needs to be done for every line in the selection. (If no selection, do it for the current line only.) You may also want to handle indentation when uncommenting (if a space after // was added, remove it too). Since there’s no single “toggle comment” API, your code will have to apply the insertion or deletion operations line by line.

* **Duplicate line/selection (Ctrl+D or Cmd+Shift+D):** Duplicating means making a copy of the current line(s) and inserting it immediately below. You can achieve this by utilizing the editor’s copy-paste capabilities or manual text manipulation. One approach: if nothing is selected, treat the entire current line as the selection. You can call textarea.select\_all() on the line – there’s no direct method to select the current line, but you can do: move to line start, call start\_selection(), then move to line end, and maybe move one char forward to include the newline. Now that the line is selected, you could use Ctrl+C (copy) via textarea.input(...) with a Copy event to copy it[\[4\]](https://github.com/rhysd/tui-textarea#:~:text=,Paste%20yanked%20text). Then move the cursor to the beginning of the next line (or insert a newline if at end of file) and use Ctrl+V (paste/yank) to insert the copied content[\[4\]](https://github.com/rhysd/tui-textarea#:~:text=,Paste%20yanked%20text). This effectively duplicates the line. For multiple lines, do the same: ensure the selection encompasses full lines, copy, then move to end of selection, and paste. If working with the API directly, you could also retrieve the text of the line(s) via textarea.lines() and insert it manually. For example, let text \= textarea.lines()\[line\_index\].clone() to get the line string, then at the end of that line call insert\_newline() followed by inserting each character of text on the new line. There are multiple ways – using the yank buffer and paste may be simplest since tui-textarea already handles those operations.

* **Move line up/down (Alt+↑/↓):** This involves removing a line from its position and inserting it either above the previous line or below the next line. You can implement it by a cut-and-paste approach. For moving a single line up: if the cursor is on the first line, do nothing. Otherwise, cut the current line and then reinsert it above. Specifically, select the line (as above, or at least ensure the whole line including newline is selected) and call the Cut operation (Ctrl+X or equivalent) – this will remove the line from the buffer[\[4\]](https://github.com/rhysd/tui-textarea#:~:text=,Paste%20yanked%20text). The removed text is now in the yank buffer. Now move the cursor **up one line** (to the line that was above) and to the beginning of that line, and insert a newline *before* it (you might have to position the cursor at the start of that line and press Enter, or simpler: move to the start of the line and paste, since the cut text includes its own newline). Pasting (Ctrl+V / yank) at that position will insert the cut line above the current line[\[4\]](https://github.com/rhysd/tui-textarea#:~:text=,Paste%20yanked%20text). Adjust cursor focus as needed (you likely want the moved line to be still selected or the cursor on it after move). Moving a line down is similar but in the opposite direction: cut the line, move cursor one line *down* (so it’s now on the line below where the original was), and paste below it. This procedure can be extended to blocks of lines if a multi-line selection is active: cut the block, move the cursor, and paste it in the new location. If you prefer not to use the yank buffer, you could manually swap lines by manipulating the textarea.lines() vector (e.g., remove a line and insert it elsewhere). However, doing it via the provided text operations is safer with regard to the editor’s undo history.

* **Indent/Dedent region (Ctrl+\]\> / Ctrl+\[ or Emacs C-M-\\):** Indentation can be done by inserting or removing leading spaces. If the user has a selection spanning multiple lines, you should indent/dedent all those lines. For indent (increase indent): iterate over each line in the selection and insert a tab or spaces at the start. tui-textarea allows configuring a tab width (say 4 spaces); pressing Tab in the editor will insert spaces accordingly. However, by default, pressing Tab in a multiline selection won’t indent all lines (there’s no built-in block indent). So you handle it manually: for each line, move cursor to the line’s start and call insert\_char(' ') the appropriate number of times (e.g., 4 spaces for one indent level, or just 1 tab character if you prefer literal tabs). Dedent is the inverse: for each selected line, if it begins with a tab or some spaces, delete those. You can move cursor to start of line and call delete\_next\_char() repeatedly up to one indent level or until a non-space is encountered. Be careful not to remove characters that aren’t indentation. (Tip: you might determine the indent unit from your settings – e.g., if using 4-space soft tabs, remove up to 4 spaces if present.) There’s no direct method like indent\_region in the crate, but these per-line insertions/deletions achieve the result. For a single-line (no selection) indent/dedent, you can apply the same logic to just the current line. Catch the keys (Ctrl+\] / Ctrl+\[) and perform the space insertion or deletion as described.

### Search and Replace Shortcuts

**Incremental search (Ctrl+S / Ctrl+R) and find next (F3/Cmd+G):** tui-textarea supports regex search highlighting and navigation, but *does not bind keys for it by default*. You can leverage its search API to implement these features. First, you’ll want to provide a way for the user to enter a search query. Typically, on Ctrl+F (or / etc.), you might open a small input box (using tui-input) for the query. Once you have a query, call textarea.set\_search\_pattern(query) – this will highlight all matches of the regex/pattern in the text[\[15\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search). You can then call textarea.search\_forward(false) to jump the cursor to the first occurrence after the current position[\[16\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search). (The boolean match\_cursor parameter, if false, ensures it goes to the next match *after* the current cursor; if true and the current position is a match, it might stay.) For reverse search (Emacs Ctrl+R), do the same but use textarea.search\_back(false)[\[16\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search) to find the previous match. Because the crate’s search uses regex, this covers normal text search as well. To emulate *incremental* search like Emacs: you could update the search pattern on each keystroke in the search input field. As the user types, continuously call set\_search\_pattern and search\_forward to jump to the latest partial match. This gives a live feedback feeling.

For **Find next/previous** (e.g. F3 or Cmd+G / Shift+F3): once a search pattern is set (from a previous search), you can simply call search\_forward(false) or search\_back(false) again on those key presses to cycle through results. (If using the Mac style Cmd+G, note that in Emacs M-g g was goto line, not search; but many GUI apps use Cmd+G for find next – implement whichever semantics you need.)

**Replace / Query replace (M-% / C-M-%):** There is no built-in replace function, so you need to implement replacements manually. A simple approach for **replace** is: after setting a search pattern, on a “replace” command you prompt the user for a replacement string. Then you can perform a global or stepwise replace. For *replace all*, you could get all occurrences of the pattern (using Rust’s regex crate on the text from textarea.lines().join("\\n")), then replace them one by one. Replacing in the TextArea can be done by moving the cursor to each match location, deleting the match length, and inserting the replacement text. However, doing this from start to end will be time-consuming and might disrupt your cursor. It might be easier to construct a new String with replacements and then reset the TextArea content to it (losing undo history), or systematically use the editor’s operations in a loop from the top. For **query replace** (Emacs M-% which prompts at each occurrence), you’ll have to iterate through matches: probably combine search\_forward to jump to next match and then pause for user input (yes/no). This implies your event loop becomes modal for the replace operation. You can highlight the current match (the crate already highlights all matches) and ask the user (maybe in a prompt) whether to replace it. If yes, use deletion and insertion to swap in the replacement, then continue to next match (which, careful, text indices shift after replacement). This is complex but doable with careful indexing or always searching from the current cursor position.

In summary, **search is partially supported** (highlighting and navigation via provided methods), but **replace requires custom logic**. Use the search APIs to find occurrences, then use text editing APIs to perform the replacements.

### Marking and Selection (Region Operations)

tui-textarea supports text selection, but selection must be initiated by your code (there are no default keybindings for setting the mark or extending selection with Shift, so you implement those):

* **Set mark (start selection) – Ctrl+Space/Ctrl+@ (Emacs style):** You can map this to call textarea.start\_selection(). When this is called, it marks the current cursor position as the beginning of a selection[\[17\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20forward%20by%20one%20character). Subsequent cursor movement calls will then create a selected region between that origin and the current cursor. (The TextArea will highlight the selection and allow cut/copy on it.) Essentially, this is the Emacs “mark” behavior. There isn’t a concept of toggling “active mark” on/off beyond this; to cancel, you can call textarea.cancel\_selection() if needed[\[17\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20forward%20by%20one%20character).

* **Extend selection with Shift \+ arrows:** The crate doesn’t automatically handle Shift-modifier arrow keys, but you can implement it in the event handler. When you detect a Shift \+ arrow key event, if no selection is active yet, first call start\_selection() to begin a selection at the current cursor. Then perform the corresponding move: e.g., on Shift+Right, call textarea.move\_cursor(CursorMove::Forward) (or simply pass the Right key event to input()) – since a selection was started, the moved-to position will extend the highlight. If a selection is already active (user already did Shift+something before), you can directly move the cursor again to continue extending. Essentially, always ensure start\_selection() was called on the first Shift+arrow, and call the normal movement for subsequent ones. (Because Input now also tracks shift in newer versions, you might integrate that, but conceptually it’s the same – you manually control it.) This way you get standard Shift+arrow text selection behavior.

* **Select all (Ctrl+A for CUA, Cmd+A for Mac, or Emacs C-x h):** There is a convenient method textarea.select\_all() which will mark the entire text as selected[\[17\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20forward%20by%20one%20character). You can bind whichever shortcut you prefer to this. Be aware that by default Ctrl+A in tui-textarea is *bound to “move to start of line”* (the Emacs C-a)[\[18\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20to%20top%20of%20lines). If you want to support both Emacs and CUA styles, you might choose a different key for “select all” (for example, implement Emacs C-x h as select-all, and leave Ctrl+A as beginning-of-line). Alternatively, you could override the Ctrl+A mapping to select all text (using select\_all()), but then you lose the quick Home key behavior on Windows. This is a design choice – you could even conditionally handle it: e.g., if the user presses Ctrl+A **while a selection already exists or in a certain mode,** do select-all, otherwise Home. (However, that might be confusing.) Since select\_all() is one call, implementing this shortcut is straightforward once you decide which key combo triggers it.

* **Cancel selection:** Not a specific shortcut in your list, but note that you can always call cancel\_selection() to clear any active selection (e.g., maybe on pressing Escape). The user can then set a new mark and begin a new selection.

Finally, keep in mind that you can always customize or override the default mappings if they conflict with your desired shortcuts. The tui-textarea documentation suggests you can call its editing methods directly for custom keybindings, or even disable the built-ins. For example, you can use textarea.input\_without\_shortcuts() instead of input() to ignore the default keymap and handle everything yourself[\[19\]](https://github.com/rhysd/tui-textarea#:~:text=If%20you%20don%27t%20want%20to,inserting%2Fdeleting%20single%20characters%2C%20tabs%2C%20newlines). In most cases you won’t need to go that far – you can let the built-in shortcuts handle what they know (as listed in the first section) and intercept only the additional keys, calling the appropriate functions or performing the custom logic described above. This approach gives you the “best of both”: built-in CUA/Emacs behavior for basics, and your own implementation for the extra shortcuts.

**Implementation Guidelines**

# 1) Capture keys reliably (Crossterm)

Both `tui-textarea` and `tui-input` are designed to be driven by your event loop. Use Crossterm for input and **only act on press events** (Windows reports press+release). Also opt-in to enhanced keyboard handling so you can detect Meta/Super cleanly. ([Ratatui][1])

```rust
use crossterm::{
  event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, PushKeyboardEnhancementFlags, KeyboardEnhancementFlags}
};

fn enable_keyboard_enhancements() -> crossterm::Result<()> {
  crossterm::terminal::enable_raw_mode()?;
  // Read distinct Meta/Super instead of ESC-prefixed fallbacks
  PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES).execute()?;
  Ok(())
}

fn next_key_event() -> crossterm::Result<Option<KeyEvent>> {
  if event::poll(std::time::Duration::from_millis(10))? {
    if let Event::Key(ev) = event::read()? {
      if ev.kind == KeyEventKind::Press {
        return Ok(Some(ev));
      }
    }
  }
  Ok(None)
}
```

> Heads-up: **Cmd (⌘)** is often intercepted by macOS terminals and never reaches your app; users must bind those combos in their terminal (or use the CUA/Emacs equivalents). iTerm2 lets you remap Option/Cmd behavior in *Profiles → Keys*. ([iTerm2][2])

---

# 2) Normalize “Meta” (Alt/Option) and platform differences

Expose a config like `tui_meta_key: MetaSource = AltOrEsc | AltOnly | Cmd` and translate platform variations into a single “Meta” bit you can match on. On macOS, many users set **Option as Meta** in their terminal; otherwise Meta arrives as an ESC-prefixed sequence, which Crossterm’s enhancement flag helps disambiguate. ([Docs.rs][3])

---

# 3) Create a declarative keymap → command layer

Define a `Command` enum (move, kill, yank, transform, search…) and a `Keymap` that translates `(KeyCode, KeyModifiers, optional sequence)` to a `Command`. Keep **one place** that adapts CUA/macOS/Emacs variants to the same command.

```rust
#[derive(Clone, Copy, Debug)]
enum Command {
  // Cursor
  LineStart, LineEnd, CharLeft, CharRight, LineUp, LineDown,
  WordLeft, WordRight, SentLeft, SentRight, DocStart, DocEnd,
  ParaUp, ParaDown, Recenter, PageUp, PageDown, GotoLine,
  MatchParenForward, MatchParenBackward,

  // Edit
  DelCharForward, DelCharBackward, DelWordForward, DelWordBackward,
  KillToEol, KillRegion, CopyRegion, Yank, YankPop,
  TransposeChars, TransposeWords,
  Undo, Redo, OpenLine, IndentOrComplete, DelToBOL,

  // Transform
  UpperWord, LowerWord, CapitalizeWord, FillPara, JoinLines,

  // Markdown formatting
  Bold, Italic, Underline, Link,

  // Code editing
  ToggleComment, DupLine, MoveLineUp, MoveLineDown, IndentRegion, DedentRegion,

  // Search
  IsearchFwd, IsearchBack, QueryReplace, QueryReplaceRegex, FindNext, FindPrev,

  // Mark/Selection
  SetMark, MarkSexp, SelectAll, ExtendWithMotion,
}
```

Then register multiple bindings per command:

```rust
use KeyModifiers as M;

fn default_keymap() -> Vec<((KeyCode, M), Command)> {
  vec![
    // Cursor movement
    ((KeyCode::Home, M::NONE), Command::LineStart),
    ((KeyCode::Left, M::CONTROL), Command::WordLeft), // Ctrl+Left
    ((KeyCode::Char('a'), M::CONTROL), Command::LineStart), // C-a
    ((KeyCode::Char('e'), M::CONTROL), Command::LineEnd),   // C-e
    ((KeyCode::Char('f'), M::CONTROL), Command::CharRight), // C-f
    ((KeyCode::Char('b'), M::CONTROL), Command::CharLeft),  // C-b
    ((KeyCode::Char('n'), M::CONTROL), Command::LineDown),  // C-n
    ((KeyCode::Char('p'), M::CONTROL), Command::LineUp),    // C-p
    ((KeyCode::Right, M::ALT), Command::WordRight),         // M-f / Opt+Right
    ((KeyCode::Left,  M::ALT), Command::WordLeft),          // M-b / Opt+Left
    ((KeyCode::Char('v'), M::CONTROL), Command::PageDown),  // C-v
    ((KeyCode::Char('v'), M::ALT), Command::PageUp),        // M-v
    ((KeyCode::Char('l'), M::CONTROL), Command::Recenter),  // C-l
    // ... add the rest of the table here (CUA/macOS fallbacks too)
  ]
}
```

> Why this shape? `tui-textarea`/`tui-input` already expose editing primitives; a thin keymap lets you reuse **one action set** for both widgets and keep platform quirks out of your app logic. ([Docs.rs][4])

---

# 4) Dispatch into `tui-textarea` / `tui-input` primitives

* `tui-textarea` offers `move_cursor`, `insert_str`, `delete_char`, `delete_word`, selection helpers, and *optional regex search* via the `search` feature — ideal for isearch/query-replace. ([Docs.rs][4])
* `tui-input` is single-line; reuse the same commands subset (no multi-line/paragraph ops). ([GitHub][5])

Sketch:

```rust
fn handle_command_textarea(cmd: Command, ta: &mut tui_textarea::TextArea<'_>) {
  use tui_textarea::{CursorMove as CM};
  match cmd {
    Command::LineStart => ta.move_cursor(CM::Head),
    Command::LineEnd   => ta.move_cursor(CM::End),
    Command::CharLeft  => ta.move_cursor(CM::Back),
    Command::CharRight => ta.move_cursor(CM::Forward),
    Command::LineUp    => ta.move_cursor(CM::Up),
    Command::LineDown  => ta.move_cursor(CM::Down),
    Command::WordLeft  => ta.move_cursor(CM::WordBack),
    Command::WordRight => ta.move_cursor(CM::WordForward),
    Command::DocStart  => ta.move_cursor(CM::Top),
    Command::DocEnd    => ta.move_cursor(CM::Bottom),
    Command::DelCharForward => { ta.delete_char(); }
    Command::DelCharBackward => { ta.backspace(); }
    Command::DelWordForward => { ta.delete_word(); }
    Command::DelWordBackward => { ta.backspace_word(); }
    Command::KillToEol => { ta.kill_line(); } // implement via extension if not present
    Command::Yank => { ta.paste_from_clipboard(); } // or your kill ring
    Command::Undo => ta.undo(),
    Command::Redo => ta.redo(),
    // …and so on
    _ => {}
  }
}
```

If you switch to `input_without_shortcuts`, you can still implement (and often extend) every default key behavior by calling the public methods on `TextArea`. Below is a concise map of the operations you’re likely to remap, with the exact function (or enum variant) to call.

## Cursor movement

Call `move_cursor(...)` with a `CursorMove` variant:

* Char: `Forward`, `Back`
* Line: `Up`, `Down`, `Head` (beginning of line), `End` (end of line)
* Document: `Top`, `Bottom`
* Word: `WordForward`, `WordBack`, `WordEnd`
* Paragraph: `ParagraphForward`, `ParagraphBack`
* Absolute jump: `Jump(row, col)`
* Keep in view: `InViewport`
  Refs: `TextArea::move_cursor` and `CursorMove` enum. ([Docs.rs][1])

## Scrolling the viewport (PgUp/PgDn/C-v/M-v, etc.)

Call `scroll(...)` with a `Scrolling` variant:
`PageDown`, `PageUp`, `HalfPageDown`, `HalfPageUp`, or `Delta { rows, cols }`. ([Docs.rs][1])

## Editing & deletion

* Insert char/string/newline/tab: `insert_char`, `insert_str`, `insert_newline`, `insert_tab`
* Delete char(s):

  * backward 1 char: `delete_char`
  * forward 1 char: `delete_next_char`
  * forward N chars: `delete_str(n)`
  * delete newline at cursor (join lines): `delete_newline`
* Delete by word/line:

  * backward word: `delete_word`
  * forward word: `delete_next_word`
  * to end of line (kill): `delete_line_by_end`
  * to start of line: `delete_line_by_head`
    Refs: method list on `TextArea`. ([Docs.rs][1])

## Clipboard / kill ring–like ops

* Copy / Cut / Paste (Yank): `copy`, `cut`, `paste`
* Inspect/override yank buffer: `yank_text`, `set_yank_text`
  Refs: `TextArea` methods. ([Docs.rs][1])

## Undo / redo

* `undo`, `redo` (and history tuning via `set_max_histories`, `max_histories`) ([Docs.rs][1])

## Selection / region

* Begin selection (“set mark”): `start_selection`
* Cancel selection: `cancel_selection`
* Select all: `select_all`
* Read selection span: `selection_range`
  Refs: `TextArea` methods. ([Docs.rs][1])

## Search

* Set/clear pattern & style: `set_search_pattern`, `search_pattern`, `set_search_style`
* Jump matches: `search_forward(match_cursor: bool)`, `search_back(match_cursor: bool)`
  Refs: `TextArea` methods. ([Docs.rs][1])

## Placeholders (for “All inputs should have placeholder text”)

* Text: `set_placeholder_text`, `placeholder_text`
* Style: `set_placeholder_style`, `placeholder_style`
  Refs: `TextArea` methods. ([Docs.rs][1])

## Tabs/indent configuration (useful for indent/dedent features)

* Tab width & mode: `set_tab_length`, `tab_length`, `set_hard_tab_indent`, `hard_tab_indent`, `indent()` (returns the indent string)
  Refs: `TextArea` methods. ([Docs.rs][1])

List of operations provided, supported by tui-textarea:

| Method                                               | Operation                                       |
|------------------------------------------------------|-------------------------------------------------|
| `textarea.delete_char()`                             | Delete one character before cursor              |
| `textarea.delete_next_char()`                        | Delete one character next to cursor             |
| `textarea.insert_newline()`                          | Insert newline                                  |
| `textarea.delete_line_by_end()`                      | Delete from cursor until the end of line        |
| `textarea.delete_line_by_head()`                     | Delete from cursor until the head of line       |
| `textarea.delete_word()`                             | Delete one word before cursor                   |
| `textarea.delete_next_word()`                        | Delete one word next to cursor                  |
| `textarea.undo()`                                    | Undo                                            |
| `textarea.redo()`                                    | Redo                                            |
| `textarea.copy()`                                    | Copy selected text                              |
| `textarea.cut()`                                     | Cut selected text                               |
| `textarea.paste()`                                   | Paste yanked text                               |
| `textarea.start_selection()`                         | Start text selection                            |
| `textarea.cancel_selection()`                        | Cancel text selection                           |
| `textarea.select_all()`                              | Select entire text                              |
| `textarea.move_cursor(CursorMove::Forward)`          | Move cursor forward by one character            |
| `textarea.move_cursor(CursorMove::Back)`             | Move cursor backward by one character           |
| `textarea.move_cursor(CursorMove::Up)`               | Move cursor up by one line                      |
| `textarea.move_cursor(CursorMove::Down)`             | Move cursor down by one line                    |
| `textarea.move_cursor(CursorMove::WordForward)`      | Move cursor forward by word                     |
| `textarea.move_cursor(CursorMove::WordEnd)`          | Move cursor to next end of word                 |
| `textarea.move_cursor(CursorMove::WordBack)`         | Move cursor backward by word                    |
| `textarea.move_cursor(CursorMove::ParagraphForward)` | Move cursor up by paragraph                     |
| `textarea.move_cursor(CursorMove::ParagraphBack)`    | Move cursor down by paragraph                   |
| `textarea.move_cursor(CursorMove::End)`              | Move cursor to the end of line                  |
| `textarea.move_cursor(CursorMove::Head)`             | Move cursor to the head of line                 |
| `textarea.move_cursor(CursorMove::Top)`              | Move cursor to top of lines                     |
| `textarea.move_cursor(CursorMove::Bottom)`           | Move cursor to bottom of lines                  |
| `textarea.move_cursor(CursorMove::Jump(row, col))`   | Move cursor to (row, col) position              |
| `textarea.move_cursor(CursorMove::InViewport)`       | Move cursor to stay in the viewport             |
| `textarea.set_search_pattern(pattern)`               | Set a pattern for text search                   |
| `textarea.search_forward(match_cursor)`              | Move cursor to next match of text search        |
| `textarea.search_back(match_cursor)`                 | Move cursor to previous match of text search    |
| `textarea.scroll(Scrolling::PageDown)`               | Scroll down the viewport by page                |
| `textarea.scroll(Scrolling::PageUp)`                 | Scroll up the viewport by page                  |
| `textarea.scroll(Scrolling::HalfPageDown)`           | Scroll down the viewport by half-page           |
| `textarea.scroll(Scrolling::HalfPageUp)`             | Scroll up the viewport by half-page             |
| `textarea.scroll((row, col))`                        | Scroll down the viewport to (row, col) position |

To define your own key mappings, simply call the above methods in your code instead of `TextArea::input()` method.

---

### How to wire keys yourself

* Feed normal text keys through `input_without_shortcuts(...)` (it already handles plain chars, Tab, Enter, Backspace, Delete). For everything else, map your `crossterm` key events to the calls above (e.g., on `Ctrl+K` call `delete_line_by_end`, on `Alt+F` call `move_cursor(CursorMove::WordForward)`, on PgDn call `scroll(Scrolling::PageDown)`, etc.). ([Docs.rs][1])

If you want the “source of truth” list in one place, the docs’ **Methods** section for `TextArea` enumerates all the functions referenced above; the **`CursorMove`** and **`Scrolling`** enums enumerate every movement/scrolling target you can invoke. ([Docs.rs][1])

*(FYI, the project also documents which default keys map to which actions; if you’re replicating those bindings manually, that table is a handy reference.)* ([github.com][2])

[1]: https://docs.rs/tui-textarea/latest/tui_textarea/struct.TextArea.html "TextArea in tui_textarea - Rust"
[2]: https://github.com/rhysd/tui-textarea/issues/51?utm_source=chatgpt.com "Remove Emacs-like shortcuts from `TextArea::input` · Issue #51 - GitHub"

### Single-line input like `<input>` in HTML

To use `TextArea` for a single-line input widget like `<input>` in HTML, ignore all key mappings which inserts newline.

```rust,ignore
use crossterm::event::{Event, read};
use tui_textarea::{Input, Key};

let default_text: &str = ...;
let default_text = default_text.replace(&['\n', '\r'], " "); // Ensure no new line is contained
let mut textarea = TextArea::new(vec![default_text]);

// Event loop
loop {
    // ...

    // Using `Input` is not mandatory, but it's useful for pattern match
    // Ignore Ctrl+m and Enter. Otherwise handle keys as usual
    match read()?.into() {
        Input { key: Key::Char('m'), ctrl: true, alt: false }
        | Input { key: Key::Enter, .. } => continue,
        input => {
            textarea.input(key);
        }
    }
}

let text = textarea.into_lines().remove(0); // Get input text
```

**Summary:** Many common shortcuts are supported directly by tui-textarea and tui-input (especially for moving cursor, deleting text, clipboard, etc.), so you should use those via the provided input(event) handling[\[3\]](https://github.com/rhysd/tui-textarea#:~:text=Mappings%20Description%20,one%20word%20next%20to%20cursor)[\[20\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20down%20by%20one%20line). For the more advanced operations (sentence navigation, transposing text, case changes, formatting, multi-line operations, search/replace, etc.), you’ll leverage the crate’s API (cursor movement methods, delete/insert functions, selection start, search functions, etc.) to implement them in your event loop. By combining these APIs with some custom logic, you can achieve the full range of shortcuts in your cross-platform TUI editor.

**Sources:**

* tui-textarea README – list of default key mappings for editing and navigation[\[3\]](https://github.com/rhysd/tui-textarea#:~:text=Mappings%20Description%20,one%20word%20next%20to%20cursor)[\[20\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20down%20by%20one%20line)[\[1\]](https://github.com/rhysd/tui-textarea#:~:text=,to%20the%20end%20of%20line)[\[2\]](https://github.com/rhysd/tui-textarea#:~:text=,Scroll%20up%20by%20page)

* tui-textarea Documentation – methods for cursor movement, selection, search, scrolling, etc.[\[21\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search)[\[17\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20forward%20by%20one%20character)

* tui-input source – mapping of crossterm key events to editing operations (single-line input)[\[6\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L32-L40)[\[9\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L44-L53)

* tui-textarea README – guidance on custom key handling (input\_without\_shortcuts)[\[19\]](https://github.com/rhysd/tui-textarea#:~:text=If%20you%20don%27t%20want%20to,inserting%2Fdeleting%20single%20characters%2C%20tabs%2C%20newlines)

---

[\[1\]](https://github.com/rhysd/tui-textarea#:~:text=,to%20the%20end%20of%20line) [\[2\]](https://github.com/rhysd/tui-textarea#:~:text=,Scroll%20up%20by%20page) [\[3\]](https://github.com/rhysd/tui-textarea#:~:text=Mappings%20Description%20,one%20word%20next%20to%20cursor) [\[4\]](https://github.com/rhysd/tui-textarea#:~:text=,Paste%20yanked%20text) [\[11\]](https://github.com/rhysd/tui-textarea#:~:text=popup_placeholder) [\[12\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search) [\[13\]](https://github.com/rhysd/tui-textarea#:~:text=,page) [\[14\]](https://github.com/rhysd/tui-textarea#:~:text=Method%20Operation%20,character%20before%20cursor) [\[15\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search) [\[16\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search) [\[17\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20forward%20by%20one%20character) [\[18\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20to%20top%20of%20lines) [\[19\]](https://github.com/rhysd/tui-textarea#:~:text=If%20you%20don%27t%20want%20to,inserting%2Fdeleting%20single%20characters%2C%20tabs%2C%20newlines) [\[20\]](https://github.com/rhysd/tui-textarea#:~:text=,cursor%20down%20by%20one%20line) [\[21\]](https://github.com/rhysd/tui-textarea#:~:text=,next%20match%20of%20text%20search) GitHub \- rhysd/tui-textarea: Simple yet powerful multi-line text editor widget for ratatui and tui-rs

[https://github.com/rhysd/tui-textarea](https://github.com/rhysd/tui-textarea)

[\[5\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L26-L34) [\[6\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L32-L40) [\[7\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L35-L42) [\[8\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L52-L57) [\[9\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L44-L53) [\[10\]](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs#L30-L38) crossterm.rs

[https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs](https://github.com/sayanarijit/tui-input/blob/77634cf42c22fd68657a9c45b5ebd66559bf127a/src/backend/crossterm.rs)
