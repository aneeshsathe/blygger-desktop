// pm-lite: a tiny ProseMirror-like editor for the macro smoke test
// (scripts/browser-macro-check.sh). Like ProseMirror, it keeps its own
// model and re-renders from it: text written into the DOM directly
// (textContent, innerHTML, execCommand) is reverted. Only a trusted paste
// and `beforeinput` insertText (typing) change the model.
(function () {
  "use strict";
  function PmLite(el) {
    this.el = el;
    this.doc = "";
    this.rendering = false;
    el.setAttribute("contenteditable", "true");
    el.classList.add("pm");
    var self = this;
    el.addEventListener("beforeinput", function (e) {
      e.preventDefault();
      if (e.inputType === "insertText" && typeof e.data === "string") {
        self.insert(e.data);
      } else if (e.inputType === "insertParagraph" || e.inputType === "insertLineBreak") {
        self.insert("\n");
      } else if (e.inputType === "deleteContentBackward") {
        self.doc = self.whole() ? "" : self.doc.slice(0, -1);
        self.render();
      }
    });
    el.addEventListener("paste", function (e) {
      e.preventDefault();
      if (!e.isTrusted) return; // synthetic paste events are ignored
      var t = e.clipboardData ? e.clipboardData.getData("text/plain") : "";
      self.insert(t);
    });
    new MutationObserver(function () {
      if (!self.rendering) self.render();
    }).observe(el, { childList: true, subtree: true, characterData: true });
    this.render();
  }
  // The selection covers the whole box (select-all, as the runner does).
  PmLite.prototype.whole = function () {
    var s = window.getSelection();
    if (!s || !s.rangeCount || s.isCollapsed) return false;
    return s.toString().trim() === this.el.innerText.trim();
  };
  PmLite.prototype.insert = function (t) {
    this.doc = this.whole() ? t : this.doc + t;
    this.render();
  };
  PmLite.prototype.clear = function () {
    this.doc = "";
    this.render();
  };
  PmLite.prototype.render = function () {
    this.rendering = true;
    var el = this.el;
    while (el.firstChild) el.removeChild(el.firstChild);
    var paras = this.doc.split(/\n{2,}/);
    for (var i = 0; i < paras.length; i++) {
      if (!paras[i] && paras.length === 1) break;
      var p = document.createElement("p");
      p.textContent = paras[i];
      el.appendChild(p);
    }
    // Caret at the end.
    var r = document.createRange();
    r.selectNodeContents(el);
    r.collapse(false);
    var s = window.getSelection();
    if (document.activeElement === el && s) {
      s.removeAllRanges();
      s.addRange(r);
    }
    var self = this;
    // The observer's records for our own render are delivered after this
    // task; ignore them.
    Promise.resolve().then(function () { self.rendering = false; });
  };
  window.PmLite = PmLite;
})();
