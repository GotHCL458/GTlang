import re
import subprocess
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parent.parent.parent
VIXC0 = ROOT / "bootstrap" / "vixc0"
HOST_VIXC = ROOT / "build" / "vixc"


ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


@pytest.fixture(scope="session")
def vixc0_binary():
    if not HOST_VIXC.exists():
        pytest.skip("build/vixc is required to build vixc0")
    result = subprocess.run(
        ["make", "-C", str(ROOT / "bootstrap"), "bootstrap"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert VIXC0.exists()
    return VIXC0


def strip_ansi(text: str) -> str:
    return ANSI_RE.sub("", text)


def test_vixc0_type_error_reports_line_and_column(vixc0_binary, tmp_path):
    src = tmp_path / "bad_type.vix"
    src.write_text(
        "fn main(): i32 {\n"
        "    let x: i32 = \"bad\";\n"
        "    return 0;\n"
        "}\n"
    )

    result = subprocess.run(
        [str(vixc0_binary), "--typeinfer", str(src)],
        capture_output=True,
        text=True,
        timeout=20,
    )

    out = strip_ansi(result.stdout + result.stderr)
    assert f"{src}:2:9" in out
    assert "cannot initialize 'x' of type 'i32' with 'string'" in out
    assert "let x: i32 = \"bad\";" in out
    assert "^" in out


def test_vixc0_return_type_error_highlights_return_keyword(vixc0_binary, tmp_path):
    src = tmp_path / "bad_return.vix"
    src.write_text(
        "fn foo(): i32 {\n"
        "    return \"w\";\n"
        "}\n"
        "fn main(): i32 {\n"
        "    return 0;\n"
        "}\n"
    )

    result = subprocess.run(
        [str(vixc0_binary), "--typeinfer", str(src)],
        capture_output=True,
        text=True,
        timeout=20,
    )

    raw = result.stdout + result.stderr
    out = strip_ansi(raw)
    assert f"{src}:2:5" in out
    assert "return \"w\";" in out
    assert "^^^^^^" in out
    assert "TypeError(E3012)\x1b[0m\x1b[2m]" in raw
