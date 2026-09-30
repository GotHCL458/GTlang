(function() {
    var EXAMPLES = {
        hello: 'fn main(): i32 {\n'
            + '    print("Hello, Vix Playground!")\n'
            + '    return 0\n'
            + '}',
        fib: 'fn fib(n: i32): i32 {\n'
            + '    if n <= 1 { return n }\n'
            + '    return fib(n - 1) + fib(n - 2)\n'
            + '}\n'
            + 'fn main(): i32 {\n'
            + '    print(fib(10))\n'
            + '    return 0\n'
            + '}',
        struct: 'struct Point {\n'
            + '    x: i32,\n'
            + '    y: i32,\n'
            + '}\n'
            + 'fn main(): i32 {\n'
            + '    let p = Point { x: 3, y: 4 }\n'
            + '    print(p.x)\n'
            + '    return 0\n'
            + '}',
        match: 'fn describe(n: i32): i32 {\n'
            + '    match n {\n'
            + '        0 => { print("zero") }\n'
            + '        1 => { print("one") }\n'
            + '        _ => { print("many") }\n'
            + '    }\n'
            + '    return 0\n'
            + '}\n'
            + 'fn main(): i32 {\n'
            + '    describe(0)\n'
            + '    describe(2)\n'
            + '    return 0\n'
            + '}',
    };

    var saved = localStorage.getItem('vix-playground-code');
    var initialCode = saved && EXAMPLES[saved] ? EXAMPLES[saved] : (saved || EXAMPLES.hello);

    var editor = CodeMirror(document.getElementById('editor-container'), {
        value: initialCode,
        mode: 'text/x-vix',
        theme: 'default',
        lineNumbers: true,
        indentUnit: 4,
        tabSize: 4,
        autofocus: true,
        extraKeys: {
            'Ctrl-Enter': runCode,
            'Cmd-Enter': runCode
        }
    });

    editor.on('change', function() {
        localStorage.setItem('vix-playground-code', editor.getValue());
    });

    var statusText = document.getElementById('status-text');
    var runBtn = document.getElementById('btn-run');
    var exampleSel = document.getElementById('example-selector');
    var loadingOverlay = document.getElementById('loading-overlay');

    exampleSel.addEventListener('change', function() {
        var code = EXAMPLES[this.value];
        if (code) {
            editor.setValue(code);
            localStorage.setItem('vix-playground-code', code);
        }
    });

    var wasmModule = null;
    var outputText = '';

    function flushOutput() {
        var el = document.getElementById('output-content');
        el.textContent = outputText;
        el.scrollTop = el.scrollHeight;
    }

    function formatCompileError(raw) {
        var lines = raw.split('\n');
        for (var i = 0; i < lines.length; i++) {
            if (lines[i].includes('SyntaxError') || lines[i].includes('TypeError')) {
                var m = lines[i].match(/:(\d+):(\d+)/);
                if (m) {
                    var line = parseInt(m[1]);
                    var col = parseInt(m[2]);
                    return '行 ' + line + ', 列 ' + col + ': ' + lines[i];
                }
                return lines[i];
            }
        }
        return raw || '未知错误';
    }

    function createWasmImports(wasmMemory) {
        var env = {
            vix_putchar: function(c) {
                outputText += String.fromCharCode(c);
                if (c === 10) flushOutput();
            },
            vix_puts: function(ptr) {
                var mem = new Uint8Array(wasmMemory.buffer);
                var s = '';
                while (mem[ptr] !== 0) {
                    s += String.fromCharCode(mem[ptr]);
                    ptr++;
                }
                outputText += s + '\n';
                flushOutput();
            },
            vix_exit: function(code) {
                console.log('Program exited with code', code);
            }
        };
        return { env: env };
    }

    async function runCode() {
        var source = editor.getValue();
        var outputEl = document.getElementById('output-content');
        outputText = '';
        outputEl.textContent = '编译中...\n';

        try {
            var resultPtr = Module.ccall('compile_vix',
                'number', ['string', 'number', 'number', 'number'],
                [source, null, null, null]);

            if (!resultPtr) {
                var errPtr = Module.getValue(resultPtr + 8, 'i32');
                var errorMsg = errPtr ? Module.UTF8ToString(errPtr) : '编译失败';
                outputEl.textContent = formatCompileError(errorMsg);
                Module.ccall('free_wasm_result', null, ['number', 'number'], [null, errPtr]);
                return;
            }

            var ptr = Module.getValue(resultPtr, 'i32');
            var len = Module.getValue(resultPtr + 4, 'i32');
            var wasmBytes = Module.HEAPU8.slice(ptr, ptr + len);

            var importObj = createWasmImports(wasmModule ? wasmModule.instance.exports.memory : null);
            wasmModule = await WebAssembly.instantiate(wasmBytes, importObj);
            outputText = '';
            wasmModule.instance.exports.main();

            Module.ccall('free_wasm_result', null, ['number', 'number'], [ptr, null]);
        } catch (err) {
            outputEl.textContent += '运行时错误: ' + err.message + '\n';
        }
    }

    runBtn.disabled = true;
    statusText.textContent = '正在加载编译器 (5-10 MB)...';

    window.onVixcWasmReady = function() {
        statusText.textContent = '编译器就绪';
        runBtn.disabled = false;
        if (loadingOverlay) loadingOverlay.style.display = 'none';
    };

    document.querySelectorAll('[data-tab]').forEach(function(btn) {
        btn.addEventListener('click', function() {
            document.querySelectorAll('[data-tab]').forEach(function(b) { b.classList.remove('active'); });
            this.classList.add('active');
            var tab = this.dataset.tab;
            document.getElementById('output-content').style.display = tab === 'output' ? 'block' : 'none';
            document.getElementById('wasm-content').style.display = tab === 'wasm' ? 'block' : 'none';
        });
    });

    runBtn.addEventListener('click', runCode);
})();
