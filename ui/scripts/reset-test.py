import json,urllib.request as u
B='http://127.0.0.1:5281/store/~/v1'
d=json.load(u.urlopen(B+'/state'))
ops=[{'do':'trash','id':o['id']} for o in d['objects'] if o['type'] in('tool','run','proposal') and not o['trashed']]
ops+=[{'do':'set','id':o['id'],'fields':{'rot':0}} for o in d['objects'] if o['type']=='photo' and o['fields'].get('rot')]
u.urlopen(u.Request(B+'/do',json.dumps({'do':'batch','intent':'reset','ops':ops}).encode(),{'content-type':'application/json'}))
