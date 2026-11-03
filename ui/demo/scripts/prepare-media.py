#!/usr/bin/env python3
"""Turns the vetted raw downloads into demo/public/media and demo/media.json.
Re-encoding drops all metadata; movies lose their audio, so no licensed music
can come along with a NASA clip. Usage: prepare-media.py <raw dir> <demo dir>"""
import hashlib, html, json, os, subprocess, sys
raw, demo = sys.argv[1:3]
for d in ('thumb', 'photo', 'poster', 'movie'):
    os.makedirs(f'{demo}/public/media/{d}', exist_ok=True)
ff = lambda *a: subprocess.run(['ffmpeg', '-v', 'error', '-y', *a], check=True)
size = lambda f: tuple(map(int, subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams', 'v', '-show_entries', 'stream=width,height', '-of', 'csv=p=0', f], text=True).strip().split(',')[:2]))
dur = lambda f: float(subprocess.check_output(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', f], text=True))
sid = lambda s: hashlib.sha1(s.encode()).hexdigest()[:16]
SCALE = {'thumb': 480, 'photo': 2000}

def photo(src, title, license, author, page):
    i = sid(page)
    for kind, w in SCALE.items():
        ff('-i', src, '-frames:v', '1', '-map_metadata', '-1', '-vf', f"scale='min({w},iw)':-2", '-q:v', '4', '-f', 'image2', f'{demo}/public/media/{kind}/{i}')
    w, h = size(src)
    return dict(id=i, title=title, w=w, h=h, author=author, license=license, page=page)

nasa = json.load(open(f'{raw}/nasa-images.json'))
def orbit(key, label, n=8, extra=()):
    out = []
    for p in nasa[key][:n] + list(extra):
        t = p['title'] if not p['title'].lower().startswith(('iss', 'ehdc')) else f'{label} {len(out) + 1}'
        t = t.split(' in Western')[0].replace('STS-56 Earth observation of ', '')
        out.append(photo(f"{raw}/../{p['file']}", t[:60], 'Public domain', 'NASA', p['url']))
    return out

cands = {hashlib.sha1(p['title'].encode()).hexdigest()[:16]: p for l in json.load(open(f'{raw}/candidates.json')).values() for p in l}
def commons(ids, names):
    out = []
    for i, name in zip(ids, names):
        p = cands[i]
        assert p['license'] == 'CC0'
        out.append(photo(f'{raw}/img/{i}.jpg', name, 'CC0', html.unescape(p['author']).split('  ')[0][:40], p['page']))
    return out

albums = [
    ('Earth at night', orbit('Earth at night', 'Night pass')),
    ('Deserts from orbit', orbit('Deserts from orbit', 'Desert', extra=nasa['Coasts from orbit'])),
    ('Mountains from orbit', orbit('Mountains from orbit', 'Range')),
    ('Mountains', commons(['b53cbe2b039a4a79', '09c4095f7d6b9f2f', '4625f719a82e3929', 'd608cd80db45e9a8'], ['Mitre Peak', 'Long white cloud', 'Under snow', 'Clouds over the range'])),
    ('Forests', commons(['1a1ecce40174f3a8', '6a65bfe0d9a54314', 'cbc9bb1796e7bae0'], ['Wet bark', 'Autumn path', 'Forest church'])),
    ('Alpine sketches', commons(['096a8b984df7a691', '3aa66da87fdbd536', '561d424a36ae3ca5', '55b73eb4c4a12bbe', 'df582b6b2f5cba4f', '22b714bad9faa06b'], ['Walkers below the Aiguille', 'Snowy peak, Chamonix', 'Jungfrau, Mönch and Eiger', 'Mountains near the Harz', 'Le Chapeau, Chamonix', 'Houses below the Alps'])),
]

V = {'GSFC_20150420_Orbit_m11858_2014': ('Earth', 'Earth from orbit, 2014', 28),
     'GSFC_20140709_SDO_m11605_Firework_Flare': ('The Sun', 'Firework flare', 44),
     'GSFC_20131217_ArgoSun_m11385': ('The Sun', 'Jewel box Sun', 26),
     'MAF_20250113_LunarMarsOcc': ('The Moon', 'The Moon eats Mars', 24)}
folders = {}
for v in json.load(open(f'{raw}/nasa-videos.json')):
    if v['nasa_id'] not in V: continue
    folder, title, crf = V[v['nasa_id']]
    src = f"{raw}/../{v['file']}"
    i = sid(v['page'])
    out = f'{demo}/public/media/movie/{i}'
    ff('-i', src, '-map_metadata', '-1', '-an', '-c:v', 'libx264', '-preset', 'slow', '-crf', str(crf), '-pix_fmt', 'yuv420p', '-movflags', '+faststart', '-f', 'mp4', out)
    assert os.path.getsize(out) < 25 * 1024 * 1024, out
    d = dur(src)
    ff('-ss', str(d * .3), '-i', src, '-frames:v', '1', '-vf', 'scale=480:-2', '-q:v', '5', '-f', 'image2', f'{demo}/public/media/poster/{i}')
    chapters = [['Start', 0]] + [[f'Part {n}', int(d * (n - 1) / 3)] for n in (2, 3)] if d > 60 else [['Start', 0]]
    folders.setdefault(folder, []).append(dict(id=i, title=title, dur=round(d), chapters=chapters, page=v['page']))

manifest = {  # a few long titles were then shortened by hand in media.json
    'albums': [dict(title=t, items=items) for t, items in albums],
    'movies': dict(title='Movies', movies=[], folders=[dict(title=t, movies=m, folders=[]) for t, m in folders.items()]),
}
json.dump(manifest, open(f'{demo}/media.json', 'w'), indent=1, ensure_ascii=False)
rows = [(p['title'], a, p['author'], p['license'], p['page']) for a, items in albums for p in items]
rows += [(m['title'], f'Movies / {f}', 'NASA', 'Public domain, audio removed', m['page']) for f, ms in folders.items() for m in ms]
tr = '\n'.join(f'<tr><td>{html.escape(t)}</td><td>{html.escape(a)}</td><td>{html.escape(b)}</td><td>{html.escape(l)}</td><td><a href="{html.escape(u)}">Source</a></td></tr>' for t, a, b, l, u in rows)
open(f'{demo}/public/credits.html', 'w').write(f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Locus demo: credits</title><style>body{{font:16px/1.5 system-ui,sans-serif;max-width:60rem;margin:2rem auto;padding:0 1rem}}table{{border-collapse:collapse;width:100%}}td,th{{text-align:left;padding:.3rem .6rem;border-bottom:1px solid #ccc}}</style></head><body><h1>Credits</h1><p><a href="/">Back to the demo</a></p><p>The notes, people, music titles and everything else in this demo are invented. Every photo and movie is CC0 or public domain. NASA material is used under its media guidelines and does not imply NASA's endorsement. Images were resized and re-encoded.</p><table><tr><th>Title</th><th>Where</th><th>By</th><th>License</th><th></th></tr>
{tr}
</table></body></html>''')
print(len(rows), 'items')
