var VixcWasm = {
    _wasm_ready: false,
    _pending: [],

    init: function() {
        return new Promise(function(resolve, reject) {
            if (VixcWasm._wasm_ready) { resolve(); return; }
            VixcWasm._pending.push({ resolve: resolve, reject: reject });
        });
    }
};

Module.onProgress = function(progress) {
    var bar = document.getElementById('loading-bar');
    if (bar) bar.style.width = (progress * 100) + '%';
};

function onVixcWasmReady() {
    VixcWasm._wasm_ready = true;
    VixcWasm._pending.forEach(function(p) { p.resolve(); });
    VixcWasm._pending = [];
}
