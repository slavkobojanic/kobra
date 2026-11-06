/*
 * Syntax highlighting for K in the language book.
 *
 * mdBook bundles highlight.js 10 and runs it (from book.js) before any
 * additional-js file loads. Blocks tagged ```k are skipped at that point,
 * because the language is unknown and highlight.js leaves their text
 * untouched. This file registers K and then highlights those blocks.
 *
 * The keyword lists follow docs/compiler/grammar.md (sections 3.3 and 6).
 */
(function () {
    'use strict';

    if (typeof hljs === 'undefined') {
        return;
    }

    function k(hljs) {
        var HARD_KEYWORDS = [
            'and', 'as', 'async', 'await', 'bind', 'break', 'case', 'catch', 'class',
            'continue', 'default', 'do', 'else', 'enum', 'expect', 'extend', 'for',
            'func', 'guard', 'if', 'implements', 'import', 'in', 'init', 'interface',
            'is', 'let', 'match', 'not', 'or', 'private', 'public', 'requires', 'return',
            'rpc', 'self', 'static', 'struct', 'style', 'throw', 'throws', 'try', 'use',
            'var', 'when', 'where', 'while'
        ];

        // Contextual keywords: highlighted everywhere, which is right in the
        // overwhelming majority of book examples. `config` and `migration`
        // come from design proposals.
        var CONTEXTUAL_KEYWORDS = [
            'app', 'at', 'component', 'entity', 'failed', 'key', 'layout', 'loading',
            'metadata', 'page', 'proto', 'protocol', 'repository', 'service', 'test',
            'theme', 'variant', 'view', 'config', 'migration'
        ];

        var KEYWORDS = {
            keyword: HARD_KEYWORDS.concat(CONTEXTUAL_KEYWORDS).join(' '),
            literal: 'true false nil'
        };

        var ESCAPE = {
            className: 'subst',
            begin: /\\(?:u\{[0-9A-Fa-f]{1,6}\}|[nrt0\\"])/,
            relevance: 0
        };

        // `{{` is a literal brace inside a string, not an interpolation.
        var BRACE_ESCAPE = { begin: /\{\{/, relevance: 0 };

        var INTERPOLATION = {
            className: 'subst',
            begin: /\{/,
            end: /\}/,
            keywords: KEYWORDS,
            contains: [] // filled in below, once the other modes exist
        };

        var STRING = {
            className: 'string',
            begin: /"/,
            end: /"/,
            illegal: /\n/,
            contains: [ESCAPE, BRACE_ESCAPE, INTERPOLATION]
        };

        var NUMBER = {
            className: 'number',
            relevance: 0,
            variants: [
                { begin: /\b0x[0-9A-Fa-f_]+\b/ },
                { begin: /\b0b[01_]+\b/ },
                { begin: /\b0o[0-7_]+\b/ },
                {
                    // 16px, 1.5rem, 200ms, 2s, 50%, 45deg, 1e9, 2.5e-3, 1_000
                    begin: /\b\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?(?:px|rem|em|ch|vw|vh|vmin|vmax|dvh|svh|lvh|fr|deg|ms|s|%)?/
                }
            ]
        };

        var COLOR = {
            className: 'number',
            begin: /#(?:[0-9A-Fa-f]{8}|[0-9A-Fa-f]{6}|[0-9A-Fa-f]{4}|[0-9A-Fa-f]{3})\b/,
            relevance: 0
        };

        var ANNOTATION = {
            className: 'meta',
            begin: /@[A-Za-z_][A-Za-z0-9_]*/
        };

        // Capitalized names are types, components, pages and other kinds.
        var TYPE = {
            className: 'type',
            begin: /\b[A-Z][A-Za-z0-9_]*/,
            relevance: 0
        };

        // A leading `.name` with no receiver: an implicit member (`.primary`,
        // `.submit`, `.product(id: 42)`) or a chained modifier at the start
        // of a line (`.class(.muted)`).
        var IMPLICIT_MEMBER = {
            className: 'symbol',
            begin: /(?<![A-Za-z0-9_)\]}"?])\.[a-z_][A-Za-z0-9_]*/,
            relevance: 0
        };

        var FUNCTION = {
            className: 'function',
            beginKeywords: 'func rpc',
            end: /[(<{]/,
            returnEnd: true,
            contains: [
                { className: 'title', begin: /[A-Za-z_][A-Za-z0-9_]*/, relevance: 0 }
            ]
        };

        var DOC_COMMENT = hljs.COMMENT(/\/\/\//, /$/, {
            contains: [{ className: 'doctag', begin: /- (?:Parameters?|Returns|Throws)(?= |:)/ }]
        });

        var COMMON = [
            DOC_COMMENT,
            hljs.C_LINE_COMMENT_MODE,
            hljs.C_BLOCK_COMMENT_MODE,
            STRING,
            ANNOTATION,
            COLOR,
            NUMBER,
            FUNCTION,
            TYPE,
            IMPLICIT_MEMBER
        ];

        INTERPOLATION.contains = [STRING, NUMBER, COLOR, TYPE, IMPLICIT_MEMBER];

        return {
            name: 'K',
            aliases: ['k'],
            keywords: KEYWORDS,
            contains: COMMON
        };
    }

    hljs.registerLanguage('k', k);

    var blocks = document.querySelectorAll('code.language-k');
    for (var i = 0; i < blocks.length; i++) {
        hljs.highlightBlock(blocks[i]);
    }
})();
