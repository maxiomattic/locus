::  Pure tests for lib/store: operations, conflicts, permissions, views, tools.
::
/-  *store
/+  *test, *store
|%
++  t0  ~2026.11.1
++  mk-note
  |=  [id=@t title=@t]
  ^-  op
  [%make id 'note' (fs ~[['title' s+title]])]
++  run
  |=  [objs=(map oid object) actor=@t ops=(list op)]
  ^-  [objs=(map oid object) undo=(list op)]
  =/  r  (apply objs 1 t0 actor ops)
  ?>  ?=(%& -.r)
  p.r
++  may
  |=  [objs=(map oid object) own=? actor=@t ops=(list op)]
  ^-  ?
  =/  r  (allowed objs own actor ops)
  ?=(%& -.r)
++  title
  |=  [objs=(map oid object) id=@t]
  ^-  json
  (~(gut by fields:(~(got by objs) id)) 'title' ~)
::
++  test-make-and-set
  =/  a  (run ~ %human ~[(mk-note 'a' 'one') [%set 'a' 0 (fs ~[['title' s+'two']])]])
  ;:  weld
    (expect-eq !>(`json`s+'two') !>((title objs.a 'a')))
    (expect-eq !>(2) !>(rev:(~(got by objs.a) 'a')))
  ==
::
++  test-duplicate-make-fails
  =/  a  (run ~ %human ~[(mk-note 'a' 'one')])
  =/  r  (apply objs.a 2 t0 %human ~[(mk-note 'a' 'x')])
  (expect !>(?=(%| -.r)))
::
++  test-batch-is-atomic
  =/  r  (apply ~ 1 t0 %human ~[(mk-note 'a' 'one') [%trash 'missing']])
  (expect !>(?=(%| -.r)))
::
++  test-undo-restores-a-field
  =/  a  (run ~ %human ~[(mk-note 'a' 'one')])
  =/  b  (run objs.a %human ~[[%set 'a' 0 (fs ~[['title' s+'two'] ['extra' s+'x']])]])
  =/  c  (run objs.b %human undo.b)
  ;:  weld
    (expect-eq !>(`json`s+'one') !>((title objs.c 'a')))
    (expect !>(!(~(has by fields:(~(got by objs.c) 'a')) 'extra')))
  ==
::
++  test-stale-write-keeps-both
  =/  a  (run ~ %human ~[(mk-note 'a' 'one')])
  =/  b  (run objs.a %human ~[[%set 'a' 1 (fs ~[['title' s+'mine']])]])
  =/  c  (run objs.b %human ~[[%set 'a' 1 (fs ~[['title' s+'theirs']])]])
  =/  o  (~(got by objs.c) 'a')
  ;:  weld
    (expect-eq !>(`json`s+'mine') !>((title objs.c 'a')))
    (expect-eq !>(1) !>((lent clashes.o)))
    (expect-eq !>(`json`s+'theirs') !>(theirs:(snag 0 clashes.o)))
  ==
::
++  test-other-field-does-not-clash
  =/  a  (run ~ %human ~[(mk-note 'a' 'one')])
  =/  b  (run objs.a %human ~[[%set 'a' 1 (fs ~[['title' s+'mine']])]])
  =/  c  (run objs.b %human ~[[%set 'a' 1 (fs ~[['tag' s+'x']])]])
  (expect-eq !>(~) !>(clashes:(~(got by objs.c) 'a')))
::
++  test-resolve-keeps-theirs
  =/  a  (run ~ %human ~[(mk-note 'a' 'one')])
  =/  b  (run objs.a %human ~[[%set 'a' 1 (fs ~[['title' s+'mine']])]])
  =/  c  (run objs.b %human ~[[%set 'a' 1 (fs ~[['title' s+'theirs']])]])
  =/  d  (run objs.c %human ~[[%resolve 'a' 'title' %.y]])
  ;:  weld
    (expect-eq !>(`json`s+'theirs') !>((title objs.d 'a')))
    (expect-eq !>(~) !>(clashes:(~(got by objs.d) 'a')))
  ==
::
++  test-splice-merges-lists
  =/  a  (run ~ %human ~[[%make 'c' 'collection' (fs ~[['items' [%a ~[s+'x' s+'y']]]])]])
  =/  b  (run objs.a %human ~[[%splice 'c' 'items' ~[s+'x'] ~[[s+'y' s+'z']]]])
  =/  c  (run objs.b %human undo.b)
  ;:  weld
    (expect-eq !>(`json`[%a ~[s+'y' s+'z']]) !>((~(got by fields:(~(got by objs.b) 'c')) 'items')))
    (expect-eq !>(`json`[%a ~[s+'x' s+'y']]) !>((~(got by fields:(~(got by objs.c) 'c')) 'items')))
  ==
::
++  test-type-rejects-bad-value
  =/  dec  [%make 'type:note' 'type' (fs ~[['fields' o+(fs ~[['title' s+'text']])]])]
  =/  a  (run ~ %human ~[dec])
  =/  r  (apply objs.a 2 t0 %human ~[[%make 'n' 'note' (fs ~[['title' n+'1']])]])
  (expect !>(?=(%| -.r)))
::
++  test-agent-needs-approval
  =/  r  (allowed ~ %.y 'agent:a' ~[(mk-note 'a' 'x')])
  (expect !>(?=(%| -.r)))
::
++  test-agent-may-propose
  =/  r  (allowed ~ %.y 'agent:a' ~[[%make 'p' 'proposal' ~]])
  (expect !>(?=(%& -.r)))
::
++  test-grant-lets-agent-apply
  =/  g=op
    :*  %make  'g'  'grant'
        %-  fs
        :~  ['actor' s+'agent:a']
            ['ops' [%a ~[s+'make']]]
            ['types' [%a ~[s+'note']]]
    ==  ==
  =/  a  (run ~ %human ~[g])
  ;:  weld
    (expect !>((may objs.a %.y 'agent:a' ~[(mk-note 'a' 'x')])))
    (expect !>(!(may objs.a %.y 'agent:b' ~[(mk-note 'a' 'x')])))
    (expect !>(!(may objs.a %.y 'agent:a' ~[[%make 'q' 'photo' ~]])))
  ==
::
++  test-agent-cannot-touch-grants
  =/  r  (allowed ~ %.y 'agent:a' ~[[%make 'g' 'grant' ~]])
  (expect !>(?=(%| -.r)))
::
++  test-member-cannot-touch-grants
  ;:  weld
    (expect !>(!(may ~ %.n %human ~[[%make 'g' 'grant' ~]])))
    (expect !>((may ~ %.y %human ~[[%make 'g' 'grant' ~]])))
  ==
::
++  test-purge-scrubs-the-log
  =/  e=entry  [1 t0 ~zod %human 'x' '' ~[(mk-note 'a' 'secret') (mk-note 'b' 'ok')] ~]
  =/  l=log  (put:on *log 1 e)
  =/  s  (scrub l 'a')
  (expect-eq !>(1) !>((lent ops:(need (get:on s 1)))))
::
++  test-query
  =/  a
    %^  run  ~  %human
    :~  [%make 'a' 'note' (fs ~[['title' s+'Apple'] ['n' n+'2']])]
        [%make 'b' 'note' (fs ~[['title' s+'Bean'] ['n' n+'1']])]
        [%make 'c' 'photo' (fs ~[['title' s+'Cat'] ['n' n+'3']])]
        [%trash 'a']
    ==
  =/  q=json  (need (de:json:html '{"type":"note"}'))
  =/  a2  (run objs.a %human ~[[%restore 'a']])
  =/  r  (query objs.a2 (need (de:json:html '{"type":"note","sort":"n"}')))
  =/  w  (query objs.a2 (need (de:json:html '{"where":[{"field":"title","has":"EA"}]}')))
  ;:  weld
    (expect-eq !>(1) !>((lent (query objs.a q))))
    (expect-eq !>(`(list @t)`~['b' 'a']) !>((turn r |=(o=object id.o))))
    (expect-eq !>(`(list @t)`~['b']) !>((turn w |=(o=object id.o))))
  ==
::
++  tag-prog
  ^-  json
  %-  need  %-  de:json:html
  '''
  [{"for":{"var":"selection"},"as":"x","do":[
    {"set":{"id":{"var":"x"},"fields":{"obj":{"tag":{"var":"tag"}}}}},
    {"when":{"eq":[{"get":[{"var":"x"},"title"]},"Apple"]},
     "do":[{"make":{"type":"note","fields":{"obj":{"title":{"concat":["copy of ",{"var":"x"}]}}}}}]}
  ]}]
  '''
::
++  test-tool-runs
  =/  a  (run ~ %human ~[(mk-note 'a' 'Apple') (mk-note 'b' 'Bean')])
  =/  args=fields  (fs ~[['selection' [%a ~[s+'a' s+'b']]] ['tag' s+'red']])
  =/  r  (run-tool objs.a tag-prog args 'salt')
  ?>  ?=(%& -.r)
  =/  b  (run objs.a %human p.r)
  ;:  weld
    (expect-eq !>(3) !>((lent p.r)))
    (expect-eq !>(`json`s+'red') !>((~(got by fields:(~(got by objs.b) 'b')) 'tag')))
    (expect-eq !>(`json`s+'copy of a') !>((title objs.b 'salt-0')))
  ==
::
++  test-tool-is-repeatable
  =/  a  (run ~ %human ~[(mk-note 'a' 'Apple')])
  =/  args=fields  (fs ~[['selection' [%a ~[s+'a']]] ['tag' s+'red']])
  (expect-eq !>((run-tool objs.a tag-prog args 's')) !>((run-tool objs.a tag-prog args 's')))
::
++  test-effects-are-static
  (expect-eq !>(`(list @t)`~['make:note' 'set']) !>((sort ~(tap in (effects tag-prog)) aor)))
::
++  test-tool-cannot-exceed-can
  =/  t=object
    :*  'tool:t'  'tool'  1
        (fs ~[['program' tag-prog] ['can' [%a ~[s+'set']]]])
        ~  |  ~  'human'  t0
    ==
  (expect !>(?=(^ (check-tool ~ t))))
::
++  test-tool-examples-gate-it
  =/  good=json  (need (de:json:html '[{"args":{"selection":[],"tag":"x"},"expect":[]}]'))
  =/  bad=json  (need (de:json:html '[{"args":{"selection":[],"tag":"x"},"expect":["set"]}]'))
  =/  can=json  [%a ~[s+'set' s+'make:note']]
  =/  mk-t
    |=  ex=json
    ^-  object
    :*  'tool:t'  'tool'  1
        (fs ~[['program' tag-prog] ['can' can] ['examples' ex]])
        ~  |  ~  'human'  t0
    ==
  ;:  weld
    (expect !>(?=(~ (check-tool ~ (mk-t good)))))
    (expect !>(?=(^ (check-tool ~ (mk-t bad)))))
  ==
::
++  test-json-round-trip
  =/  o=op  [%splice 'c' 'items' ~[s+'x'] ~[[~ s+'z']]]
  (expect-eq !>(`(unit op)``o) !>((op-d:dejs (op-j:enjs o))))
--
