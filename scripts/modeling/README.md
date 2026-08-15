# 3D Modeling Scripts

Parametric Python generators for printable Kivo hardware live in this directory.
Run them from the repository root so their default input and output paths resolve
correctly.

| Generator | Output |
|---|---|
| `integrated_workstation.py` | Integrated workstation shell, sloped panel, bottom cover, standalone controller-cradle test module, and side-mount handset base |
| `telephone_handset_switch_base.py` | Standalone telephone handset switch base |
| `macro_pad_variants.py` | Macro-pad enclosure size variants |

Each file is a standalone PEP 723 script. For example:

```bash
uv run --script scripts/modeling/integrated_workstation.py
uv run --script scripts/modeling/telephone_handset_switch_base.py
uv run --script scripts/modeling/macro_pad_variants.py
```
