"""Use the installed house renderer without changing its global model policy."""
import importlib.util
import json
from pathlib import Path

path = Path.home() / '.claude/scripts/graph/render_graph.py'
spec = importlib.util.spec_from_file_location('akm_house_graph', path)
renderer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(renderer)
renderer.load_models = lambda routing=None: {
    'openai/gpt-6.1-sol': ['GPT-6.1 Sol', 'Main session', 'self'],
    'openai/gpt-6.1-terra': ['GPT-6.1 Terra', 'Fresh read-only review', '0'],
}
work = Path(__file__).parent
page = renderer.render(json.loads((work / 'graph.json').read_text(encoding='utf-8')))
page = page.replace('</style>', '.files { flex-direction: column; align-items: stretch; } .files code { width: 100%; white-space: normal; }\n</style>', 1)
(work / 'graph.html').write_text(page, encoding='utf-8')
print(f'Rendered {work / "graph.html"}')
