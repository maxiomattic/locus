::  %store: the home of the clan's objects, or a client's copy of them.
::
::  A ship with no upstream is home: it orders every operation and checks
::  permissions. A client applies operations at once, sends them home, and
::  replays anything unconfirmed on top of home's order, so it keeps working
::  offline and its changes stay provisional until home accepts them.
::
/-  *store
/+  default-agent, dbug, server, *store
::
|%
+$  card  card:agent:gall
--
=|  state-0
=*  state  -
%-  agent:dbug
^-  agent:gall
=<
|_  =bowl:gall
+*  this  .
    def   ~(. (default-agent this %|) bowl)
    cor   ~(. +> [bowl ~])
::
++  on-init
  ^-  (quip card _this)
  =^  cards  state  abet:init:cor
  [cards this]
::
++  on-save  !>(state)
++  on-load
  |=  old=vase
  ^-  (quip card _this)
  =.  state  !<(state-0 old)
  =^  cards  state  abet:init:cor
  [cards this]
::
++  on-poke
  |=  [=mark =vase]
  ^-  (quip card _this)
  =^  cards  state  abet:(poke:cor mark vase)
  [cards this]
::
++  on-watch
  |=  =path
  ^-  (quip card _this)
  =^  cards  state  abet:(watch:cor path)
  [cards this]
::
++  on-leave  on-leave:def
++  on-peek
  |=  =path
  ^-  (unit (unit cage))
  ?+  path  ~
    [%x %state ~]   ``json+!>(state-j:cor)
    [%x %ver ~]     ``noun+!>(ver)
    [%x %seq ~]     ``noun+!>(seq)
    [%x %object @ ~]
      =/  o  (~(get by objects) i.t.t.path)
      ?~  o  [~ ~]
      ``json+!>((object-j:enjs u.o))
  ==
::
++  on-agent
  |=  [=wire =sign:agent:gall]
  ^-  (quip card _this)
  =^  cards  state  abet:(agent:cor wire sign)
  [cards this]
::
++  on-arvo
  |=  [=wire =sign-arvo]
  ^-  (quip card _this)
  ?:  ?=([%eyre *] wire)  `this
  ?.  ?=([%retry ~] wire)  (on-arvo:def wire sign-arvo)
  =^  cards  state  abet:follow:cor
  [cards this]
::
++  on-fail  on-fail:def
--
::
|_  [=bowl:gall cards=(list card)]
++  abet  [(flop cards) state]
++  cor   .
++  emit  |=(=card cor(cards [card cards]))
++  emil  |=(caz=(list card) cor(cards (welp (flop caz) cards)))
::
++  init
  ^+  cor
  (emit [%pass /eyre/store %arvo %e %connect [~ /store] %store])
::
++  poke
  |=  [=mark =vase]
  ^+  cor
  ?+  mark  ~|(bad-mark+mark !!)
      %handle-http-request
    (serve !<([@ta inbound-request:eyre] vase))
  ::
      %json
    ?>  =(our src):bowl
    =^  res  cor  (act !<(json vase))
    ?:  ?=(%| -.res)  ~|(p.res !!)
    cor
  ::
      %noun
    (ship-poke ;;(wire-poke q.vase))
  ==
::
++  home  =(~ upstream)
::
::  ---- the action surface, used by HTTP and pokes alike ----
::
++  act
  |=  jon=json
  ^-  [(each json @t) _cor]
  =/  m  (mk jon)
  =/  t  |=(k=@t =/(v (~(gut by m) k ~) ?:(?=([%s *] v) p.v '')))
  ?+    (t 'do')  [[%| 'unknown action'] cor]
      %batch
    =/  ops  (ops-d:dejs (~(gut by m) 'ops' ~))
    ?~  ops  [[%| 'bad operations'] cor]
    (submit [(t 'cid') (actor-of t) (t 'intent') u.ops])
  ::
      %propose
    =/  id=@t  (rap 3 ~['proposal:' (scot %uv (end [3 8] eny.bowl))])
    =/  f=fields
      %-  fs
      :~  ['actor' s+(actor-of t)]
          ['intent' s+(t 'intent')]
          ['ops' (~(gut by m) 'ops' ~)]
          ['status' s+'open']
      ==
    (submit ['' (actor-of t) (t 'intent') [%make id 'proposal' f]~])
  ::
      %approve  (decide (t 'id') &)
      %reject   (decide (t 'id') |)
  ::
      %revert
    =/  e  (get:on log (num (~(gut by m) 'seq' ~)))
    ?~  e  [[%| 'no such entry'] cor]
    (submit ['' 'human' (cat 3 'undo: ' intent.u.e) undo.u.e])
  ::
      %run-tool
    =/  o  (~(get by objects) (t 'id'))
    ?~  o  [[%| 'no such tool'] cor]
    ?^  bad=(check-tool objects u.o)  [[%| u.bad] cor]
    =/  args=fields  (mk (~(gut by m) 'args' ~))
    =/  r  (run-tool objects (~(gut by fields.u.o) 'program' ~) args (scot %uv (end [3 6] eny.bowl)))
    ?:  ?=(%| -.r)  [[%| p.r] cor]
    =/  who=@t  (rap 3 ~['tool:' id.u.o])
    (submit [(t 'cid') ?:(=('' (t 'actor')) 'human' (t 'actor')) (rap 3 ~[(field-text u.o 'name') ' (' who ')']) p.r])
  ::
      %try-tool
    =/  prog=json  (~(gut by m) 'program' ~)
    =/  over  (over-reach (~(gut by m) 'can' ~) prog)
    ?^  over  [[%| (cat 3 'program exceeds what it declares: ' i.over)] cor]
    =/  r  (run-tool objects prog (mk (~(gut by m) 'args' ~)) 'try')
    ?:  ?=(%| -.r)  [[%| p.r] cor]
    :_  cor
    :-  %&
    %-  pairs:enjs:format
    :~  ops+[%a (turn p.r op-j:enjs)]
        can+[%a (turn ~(tap in (effects prog)) |=(e=@t s+e))]
    ==
  ::
      %check-tool
    =/  o  (~(get by objects) (t 'id'))
    ?~  o  [[%| 'no such tool'] cor]
    =/  bad  (check-tool objects u.o)
    [[%& (pairs:enjs:format ~[ok+b+?=(~ bad) why+s+(fall bad '')])] cor]
  ::
      %query
    :_  cor
    [%& [%a (turn (query objects (~(gut by m) 'q' ~)) object-j:enjs)]]
  ::
      %follow
    =/  who  (slaw %p (t 'ship'))
    ?~  who  [[%| 'bad ship'] cor]
    =.  upstream  who
    =.  seq  0
    =.  log  ~
    =.  base  ~
    [[%& ~] follow]
  ::
      %member
    =/  who  (slaw %p (t 'ship'))
    ?~  who  [[%| 'bad ship'] cor]
    =.  members
      ?:  =([%b |] (~(gut by m) 'add' [%b &]))  (~(del in members) u.who)
      (~(put in members) u.who)
    [[%& ~] cor]
  ==
::
++  actor-of
  |=  t=$-(@t @t)
  ^-  @t
  =/  a  (t 'actor')
  ?:(=('' a) 'human' a)
::
::  Take a change from this ship: order it at home, or apply it here and
::  send it home.
::
++  submit
  |=  =batch
  ^-  [(each json @t) _cor]
  ?:  home
    =/  r  (commit our.bowl batch)
    ?:  ?=(%| -.p.r)  [[%| p.p.r] cor]
    [[%& (numb:enjs:format p.p.r)] q.r]
  =/  bad  (allowed objects & actor.batch ops.batch)
  ?:  ?=(%| -.bad)  [[%| p.bad] cor]
  =/  r  (apply objects seq now.bowl actor.batch ops.batch)
  ?:  ?=(%| -.r)  [[%| p.r] cor]
  =.  objects  objs.p.r
  =.  pending  (snoc pending batch)
  =.  cor  (touch ops.batch)
  :-  [%& ~]
  (emit (push-card [%push batch]))
::
++  push-card
  |=  p=wire-poke
  ^-  card
  [%pass /push/(scot %t ?-(-.p %push cid.batch.p, %decide id.p)) %agent [(need upstream) %store] %poke %noun !>(p)]
::
++  decide
  |=  [id=@t ok=?]
  ^-  [(each json @t) _cor]
  ?.  home
    [[%& ~] (emit (push-card [%decide id ok]))]
  =/  r  (settle id ok)
  ?:  ?=(%| -.p.r)  [[%| p.p.r] cor]
  [[%& ~] q.r]
::
++  settle
  |=  [id=@t ok=?]
  ^-  (pair (each ~ @t) _cor)
  =/  o  (~(get by objects) id)
  ?~  o  [[%| 'no such proposal'] cor]
  ?.  =('proposal' type.u.o)  [[%| 'not a proposal'] cor]
  ?.  =('open' (field-text u.o 'status'))  [[%| 'already decided'] cor]
  =/  ops=(list op)
    ?.  ok  ~
    (fall (proposal-ops u.o) ~)
  =/  mark=op  [%set id 0 (fs ~[['status' s+?:(ok 'applied' 'rejected')]])]
  =/  r
    %+  commit  our.bowl
    :*  ''
        'human'
        ?:(ok (rap 3 ~['approved: ' (field-text u.o 'intent')]) (rap 3 ~['rejected: ' (field-text u.o 'intent')]))
        (snoc ops mark)
    ==
  ?:  ?=(%| -.p.r)  [[%| p.p.r] q.r]
  [[%& ~] q.r]
::
::  Order a batch at home.
::
++  commit
  |=  [who=ship =batch]
  ^-  (pair (each @ud @t) _cor)
  =/  r  (record who batch)
  ?:  ?=(%| -.p.r)  r
  [p.r react:q.r]
::
++  record
  |=  [who=ship =batch]
  ^-  (pair (each @ud @t) _cor)
  =/  ok  (allowed objects =(who our.bowl) actor.batch ops.batch)
  ?:  ?=(%| -.ok)  [[%| p.ok] cor]
  =/  next  +(seq)
  =/  r  (apply objects next now.bowl actor.batch ops.batch)
  ?:  ?=(%| -.r)  [[%| p.r] cor]
  =/  =entry  [next now.bowl who actor.batch intent.batch cid.batch ops.batch undo.p.r]
  =.  objects  objs.p.r
  =.  seq  next
  =.  log  (put:on log next entry)
  =.  log  (purged log ops.batch)
  =.  cor  (touch ops.batch)
  =.  cor  (publish [%entry entry])
  [[%& next] cor]
::
++  purged
  |=  [l=^log ops=(list op)]
  ^-  ^log
  ?~  ops  l
  ?.  ?=(%purge -.i.ops)  $(ops t.ops)
  $(ops t.ops, l (scrub l id.i.ops))
::
::  Run reactions whose view result changed. A reaction proposes; it applies
::  by itself only when a grant lets its tool do so.
::
++  react
  ^+  cor
  =/  vs=(list object)  (reactions objects)
  =/  c=_cor  cor
  |-
  ?~  vs  quiet:c
  =/  h  (view-hash objects.c i.vs)
  =/  old  (~(get by reacted.c) id.i.vs)
  =.  reacted.c  (~(put by reacted.c) id.i.vs h)
  ?:  |(?=(~ old) =(u.old h))  $(vs t.vs)
  =/  tool  (~(get by objects.c) (field-text i.vs 'react'))
  ?~  tool  $(vs t.vs)
  ?^  (check-tool objects.c u.tool)  $(vs t.vs)
  =/  args=fields
    (fs ~[['view' [%a (turn (query objects.c (~(gut by fields.i.vs) 'query' ~)) |=(o=object [%s id.o]))]]])
  =/  r  (run-tool objects.c (~(gut by fields.u.tool) 'program' ~) args (scot %uv (end [3 6] eny.bowl)))
  ?:  ?=(%| -.r)  $(vs t.vs)
  ?~  p.r  $(vs t.vs)
  =/  actor=@t  (rap 3 ~['tool:' id.u.tool])
  =/  ok  (allowed objects.c & actor p.r)
  =/  ops=(list op)
    ?:  ?=(%& -.ok)  p.r
    :_  ~
    :^  %make  (rap 3 ~['proposal:' (scot %uv (end [3 8] (shax h)))])  'proposal'
    %-  fs
    :~  ['actor' s+actor]
        ['intent' s+(field-text u.tool 'name')]
        ['ops' [%a (turn p.r op-j:enjs)]]
        ['status' s+'open']
    ==
  =/  done  (record:c our.bowl ['' actor (field-text u.tool 'name') ops])
  $(vs t.vs, c q.done)
::  What reactions just wrote cannot set off another reaction, so a reaction
::  always ends, even one whose tool changes the view it watches.
::
++  quiet
  ^+  cor
  =/  vs=(list object)  (reactions objects)
  |-
  ?~  vs  cor
  =.  reacted  (~(put by reacted) id.i.vs (view-hash objects i.vs))
  $(vs t.vs)
::
++  touch
  |=  ops=(list op)
  ^+  cor
  =.  ver  +(ver)
  =.  touched
    %+  roll  ops
    |:  [o=*op t=touched]
    (~(put by t) (id-of o) ver)
  cor
::
++  publish
  |=  f=wire-fact
  ^+  cor
  =/  paths=(list path)
    %~  tap  in
    %-  silt
    %+  murn  ~(val by sup.bowl)
    |=  [ship p=path]
    ?.  ?=([%live *] p)  ~
    `p
  ?~  paths  cor
  (emit [%give %fact paths %noun !>(f)])
::
::  ---- ship to ship ----
::
++  ship-poke
  |=  p=wire-poke
  ^+  cor
  ?>  ?|  =(our src):bowl
          (~(has in members) src.bowl)
      ==
  ?-    -.p
      %push
    =/  r  (commit src.bowl batch.p)
    ?:  ?=(%| -.p.r)  ~|(p.p.r !!)
    q.r
  ::
      %decide
    ?>  =(our src):bowl
    =/  r  (settle [id ok]:p)
    q.r
  ==
::
++  watch
  |=  =path
  ^+  cor
  ?+    path  ~|(bad-watch+path !!)
      [%http-response *]  cor
  ::
      [%live @ ~]
    ?>  home
    ?>  ?|  =(our src):bowl
            (~(has in members) src.bowl)
        ==
    =/  from=@ud  (slav %ud i.t.path)
    =/  tail=(list [k=@ud e=entry])
      (tap:on (lot:on log `from ~))
    ?:  |(=(0 from) (gth from seq))
      (emit [%give %fact ~ %noun !>(`wire-fact`[%snap seq objects])])
    %-  emil
    %+  turn  tail
    |=  [@ud e=entry]
    ^-  card
    [%give %fact ~ %noun !>(`wire-fact`[%entry e])]
  ==
::
++  follow
  ^+  cor
  ?~  upstream  cor
  (emit [%pass /live %agent [u.upstream %store] %watch /live/(scot %ud seq)])
::
++  agent
  |=  [=wire =sign:agent:gall]
  ^+  cor
  ?+    wire  cor
      [%push @ ~]
    ?.  ?=(%poke-ack -.sign)  cor
    ?~  p.sign  cor
    =/  cid=@t  (slav %t i.t.wire)
    =/  why=@t
      ?~  u.p.sign  'refused'
      (crip ~(ram re (flop-tank u.p.sign)))
    =.  pending  (skip pending |=(b=batch =(cid cid.b)))
    =.  cor  (refuse cid why)
    rebuild
  ::
      [%live ~]
    ?+  -.sign  cor
        %kick  follow
        %watch-ack
      ?~  p.sign  cor
      (emit [%pass /retry %arvo %b %wait (add now.bowl ~s30)])
        %fact
      ?>  =(src.bowl (need upstream))
      (fact ;;(wire-fact q.q.cage.sign))
    ==
  ==
::
++  refuse
  |=  [cid=@t why=@t]
  ^+  cor
  =/  next=(list [cid=@t why=@t])  [[cid why] rejected]
  =.  rejected  (scag 50 next)
  cor
::
++  flop-tank
  |=  t=tang
  ^-  tank
  ?~  t  leaf+"refused"
  i.t
::
++  fact
  |=  f=wire-fact
  ^+  cor
  ?-    -.f
      %snap
    =.  base  objects.f
    =.  seq  seq.f
    =/  ids=(list oid)  ~(tap in ~(key by objects.f))
    =.  ver  +(ver)
    =.  touched  (~(gas by touched) (turn ids |=(i=oid [i ver])))
    rebuild
  ::
      %entry
    =*  e  entry.f
    ?.  =(seq.e +(seq))
      ?:  (lte seq.e seq)  cor
      =.  cor  (emit [%pass /live %agent [(need upstream) %store] %leave ~])
      follow
    =/  r  (apply base seq.e at.e actor.e ops.e)
    ?:  ?=(%| -.r)  ~|(p.r !!)
    =.  base  objs.p.r
    =.  seq  seq.e
    =.  log  (put:on log seq.e e)
    =.  log  (purged log ops.e)
    =.  pending  (skip pending |=(b=batch &(!=('' cid.b) =(cid.b cid.e))))
    =.  cor  (touch ops.e)
    rebuild
  ==
::
::  Replay what home has not confirmed on top of its order. A batch that no
::  longer applies is set aside with its reason, not lost silently.
::
++  rebuild
  ^+  cor
  =/  view  base
  =/  keep=(list batch)  ~
  =/  ls  pending
  |-
  ?~  ls
    =.  objects  view
    =.  pending  (flop keep)
    =.  ver  +(ver)
    cor
  =/  r  (apply view seq now.bowl actor.i.ls ops.i.ls)
  ?:  ?=(%| -.r)
    =.  cor  (refuse cid.i.ls p.r)
    $(ls t.ls)
  $(ls t.ls, view objs.p.r, keep [i.ls keep])
::
::  ---- http ----
::
++  state-j
  ^-  json
  %-  pairs:enjs:format
  :~  ver+(numb:enjs:format ver)
      seq+(numb:enjs:format seq)
      home+s+?~(upstream (scot %p our.bowl) (scot %p u.upstream))
      ship+s+(scot %p our.bowl)
      pending+(numb:enjs:format (lent pending))
      objects+[%a (turn ~(val by objects) object-j:enjs)]
  ==
::
++  changes-j
  |=  since=@ud
  ^-  json
  =/  ids=(list oid)
    %+  murn  ~(tap by touched)
    |=  [i=oid v=@ud]
    ?:((gth v since) `i ~)
  %-  pairs:enjs:format
  :~  ver+(numb:enjs:format ver)
      seq+(numb:enjs:format seq)
      pending+(numb:enjs:format (lent pending))
      :-  'rejected'
      :-  %a
      %+  turn  rejected
      |=([c=@t w=@t] (pairs:enjs:format ~[cid+s+c why+s+w]))
      objects+[%a (murn ids |=(i=oid (bind (~(get by objects) i) object-j:enjs)))]
      gone+[%a (murn ids |=(i=oid ?:((~(has by objects) i) ~ `s+i)))]
  ==
::
++  log-j
  |=  since=@ud
  ^-  json
  =/  ls  (tap:on (lot:on log `since ~))
  [%a (turn (scag 500 ls) |=([@ud e=entry] (entry-j:enjs e)))]
::
++  arg
  |=  [args=(list [key=@t value=@t]) k=@t]
  ^-  @ud
  ?~  args  0
  ?:  =(key.i.args k)  (fall (rush value.i.args dem) 0)
  $(args t.args)
::
++  serve
  |=  [eyre-id=@ta req=inbound-request:eyre]
  ^+  cor
  =/  line  (parse-request-line:server url.request.req)
  ?.  authenticated.req  (give eyre-id 401 s+'unauthorized')
  =/  method  method.request.req
  =*  args  args.line
  ?+    site.line  (give eyre-id 404 s+'not found')
      [%store %~.~ %v1 %state ~]    (give eyre-id 200 state-j)
      [%store %~.~ %v1 %changes ~]  (give eyre-id 200 (changes-j (arg args 'since')))
      [%store %~.~ %v1 %log ~]      (give eyre-id 200 (log-j (arg args 'since')))
      [%store %~.~ %v1 %links @ ~]
    =/  id=@t  (crip (wood-decode i.t.t.t.t.site.line))
    (give eyre-id 200 [%a (turn (backlinks objects id) |=(i=oid s+i))])
  ::
      [%store %~.~ %v1 %do ~]
    ?.  =(%'POST' method)  (give eyre-id 405 s+'use POST')
    ?~  body.request.req  (give eyre-id 400 s+'no body')
    =/  jon  (de:json:html q.u.body.request.req)
    ?~  jon  (give eyre-id 400 s+'bad json')
    =^  res  cor  (act u.jon)
    ?:  ?=(%| -.res)
      (give eyre-id 409 (pairs:enjs:format ~[ok+b+| error+s+p.res]))
    (give eyre-id 200 (pairs:enjs:format ~[ok+b+& result+p.res]))
  ==
::
++  wood-decode
  |=  t=@t
  ^-  tape
  (trip t)
::
++  give
  |=  [eyre-id=@ta code=@ud jon=json]
  ^+  cor
  =/  data=octs  (as-octs:mimes:html (en:json:html jon))
  =/  p=path  /http-response/[eyre-id]
  %-  emil
  :~  [%give %fact ~[p] %http-response-header !>(`response-header:http`[code ~[['content-type' 'application/json']]])]
      [%give %fact ~[p] %http-response-data !>(`(unit octs)``data)]
      [%give %kick ~[p] ~]
  ==
--
