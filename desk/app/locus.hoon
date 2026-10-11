::  %locus — clan identity, named functions, OS approval, audit.
::
/-  loc=locus
/+  default-agent, dbug, server
/+  locus-json
/*  openapi-spec  %json  /app/locus/openapi/json
::
|%
+$  card  card:agent:gall
+$  current-state  state-0:loc
--
=|  current-state
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
  =^  cards  state
    abet:init:cor
  [cards this]
::
++  on-save  !>(state)
++  on-load
  |=  old=vase
  ^-  (quip card _this)
  =^  cards  state
    abet:(load:cor old)
  [cards this]
::
++  on-poke
  |=  [=mark =vase]
  ^-  (quip card _this)
  =^  cards  state
    abet:(poke:cor mark vase)
  [cards this]
::
++  on-watch
  |=  =path
  ^-  (quip card _this)
  ?+  path  (on-watch:def path)
    [%http-response *]  `this
  ==
::
++  on-leave  on-leave:def
++  on-peek
  |=  =path
  ^-  (unit (unit cage))
  ?+  path  ~
      [%x %v0 %api-key ~]
    ``json+!>(?~(api-key ~ s+u.api-key))
      [%x %members ~]
    ``json+!>((members:enjs:locus-json members))
      [%x %audit ~]
    ``json+!>((audits:enjs:locus-json log))
      [%x %pending ~]
    ``json+!>((pending:enjs:locus-json jobs))
      [%x %jobs ~]
    ``json+!>((jobs:enjs:locus-json jobs))
      [%x %has-key @ ~]
    =/  who=@p  (slav %p i.t.t.path)
    =/  has=?
      (lien ~(tap by keys) |=([k=@t s=ship] =(s who)))
    ``json+!>(b+has)
  ==
::
++  on-agent  on-agent:def
++  on-arvo
  |=  [=wire =sign-arvo]
  ^-  (quip card _this)
  ?.  ?=([%eyre *] wire)  (on-arvo:def wire sign-arvo)
  `this
++  on-fail  on-fail:def
--
::
|_  [=bowl:gall cards=(list card)]
++  abet  [(flop cards) state]
++  cor   .
++  emit  |=(=card cor(cards [card cards]))
++  emil  |=(caz=(list card) cor(cards (welp (flop caz) cards)))
++  give  |=(=gift:agent:gall (emit %give gift))
::
++  init
  ^+  cor
  =.  api-key  `(scot %uv eny.bowl)
  =.  next-id  1
  =.  keys  *(map @t ship)
  =.  members
    %+  ~(put by *(map ship member:loc))
      our.bowl
    [our.bowl 'owner' %owner]
  (emit [%pass /eyre/locus %arvo %e %connect [~ /locus] %locus])
::
++  load
  |=  =vase
  ^+  cor
  =/  tried=(each state-0:loc tang)
    (mule |.(!<(state-0:loc vase)))
  ?:  ?=(%| -.tried)  init
  =.  state  p.tried
  (emit [%pass /eyre/locus %arvo %e %connect [~ /locus] %locus])
::
++  poke
  |=  [=mark =vase]
  ^+  cor
  ?+  mark  ~|(bad-mark+mark !!)
      %handle-http-request
    (serve-http !<([eyre-id=@ta =inbound-request:eyre] vase))
      %json
    ?>  =(our.bowl src.bowl)
    =^  jon  cor  (run-json our.bowl !<(json vase))
    cor
  ==
::
++  serve-http
  |=  [eyre-id=@ta =inbound-request:eyre]
  ^+  cor
  =/  =request-line:server
    (parse-request-line:server url.request.inbound-request)
  =*  site  site.request-line
  =/  method=@tas  method.request.inbound-request
  ?:  &(=(site ~[%locus %openapi]) =(ext.request-line `%json))
    (give-http eyre-id 200 'application/json' (en:json:html openapi-spec))
  ?.  (request-authorized inbound-request)
    (http-error eyre-id 401 'unauthorized')
  =/  caller=ship  (need (caller-of inbound-request))
  ?:  ?=([%locus %~.~ %v1 %whoami ~] site)
    ?.  =(%'GET' method)  (http-error eyre-id 405 'method not allowed')
    (give-json eyre-id (whoami-json caller))
  ?:  ?=([%locus %~.~ %v1 %members ~] site)
    ?.  =(%'GET' method)  (http-error eyre-id 405 'method not allowed')
    (give-json eyre-id (ok:enjs:locus-json [members+(members:enjs:locus-json members)]~))
  ?:  ?=([%locus %~.~ %v1 %audit ~] site)
    ?.  =(%'GET' method)  (http-error eyre-id 405 'method not allowed')
    (give-json eyre-id (ok:enjs:locus-json [audit+(audits:enjs:locus-json log)]~))
  ?:  ?=([%locus %~.~ %v1 %pending ~] site)
    ?.  =(%'GET' method)  (http-error eyre-id 405 'method not allowed')
    (give-json eyre-id (ok:enjs:locus-json [pending+(pending:enjs:locus-json jobs)]~))
  ?:  ?=([%locus %~.~ %v1 %jobs ~] site)
    ?.  =(%'GET' method)  (http-error eyre-id 405 'method not allowed')
    (give-json eyre-id (ok:enjs:locus-json [jobs+(jobs:enjs:locus-json jobs)]~))
  ?:  ?=([%locus %~.~ %v1 %call ~] site)
    ?.  =(%'POST' method)  (http-error eyre-id 405 'method not allowed')
    ?~  body.request.inbound-request
      (http-error eyre-id 400 'missing body')
    ?~  jon=(de:json:html q.u.body.request.inbound-request)
      (http-error eyre-id 400 'invalid json')
    =^  out=json  cor  (run-json caller u.jon)
    (give-json eyre-id out)
  (http-error eyre-id 404 'not found')
::
++  give-http
  |=  [eyre-id=@ta code=@ud ct=@t body=@t]
  ^+  cor
  =/  data=octs  (as-octs:mimes:html body)
  %-  emil
  :~  [%give %fact [/http-response/[eyre-id]]~ %http-response-header !>(`response-header:http`[code ~[['content-type' ct]]])]
      [%give %fact [/http-response/[eyre-id]]~ %http-response-data !>(`data)]
      [%give %kick [/http-response/[eyre-id]]~ ~]
  ==
::
++  http-error
  |=  [eyre-id=@ta code=@ud message=@t]
  (give-http eyre-id code 'text/plain' message)
::
++  give-json
  |=  [eyre-id=@ta =json]
  (give-http eyre-id 200 'application/json' (en:json:html json))
::
++  get-api-key-header
  |=  req=inbound-request:eyre
  ^-  (unit @t)
  =/  hdrs=(list [key=@t value=@t])  header-list.request.req
  |-  ^-  (unit @t)
  ?~  hdrs  ~
  ?:  =((cass (trip key.i.hdrs)) "x-api-key")  `value.i.hdrs
  $(hdrs t.hdrs)
::
++  caller-of
  |=  req=inbound-request:eyre
  ^-  (unit ship)
  =/  hdr=(unit @t)  (get-api-key-header req)
  ?^  hdr
    ?:  &(?=(^ api-key) =(u.hdr u.api-key))  `our.bowl
    (~(get by keys) u.hdr)
  ?:  authenticated.req  `our.bowl
  ~
::
++  request-authorized
  |=  req=inbound-request:eyre
  ^-  ?
  ?=(^ (caller-of req))
::
++  whoami-json
  |=  caller=ship
  ^-  json
  ?~  m=(~(get by members) caller)
    (err:enjs:locus-json 'unknown-principal' 'not a member')
  %-  ok:enjs:locus-json
  :~  ship+s+(scot %p who.u.m)
      name+s+name.u.m
      role+s+role.u.m
  ==
::
++  run-json
  |=  [caller=ship jon=json]
  ^-  [json _cor]
  =/  parsed=(each [fn=@t as=(unit @p) args=json] tang)
    (mule |.((parse-call jon)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:locus-json 'bad-request' 'malformed call') cor]
  =/  [fn=@t as=(unit @p) args=json]  p.parsed
  =/  actor=ship  caller
  =/  as-err=(unit @t)
    ?~  as  ~
    ?.  =(caller our.bowl)
      `'only the owner session may impersonate; use your own API key'
    ~
  ?^  as-err
    [(err:enjs:locus-json 'not-authorized' u.as-err) cor]
  =?  actor  ?=(^ as)  u.as
  =/  who=(unit member:loc)  (~(get by members) actor)
  ?~  who
    [(err:enjs:locus-json 'unknown-principal' 'ship is not a clan member') cor]
  =/  gate=(unit @t)  (actor-gate u.who as)
  ?^  gate
    [(err:enjs:locus-json 'not-authorized' u.gate) cor]
  (dispatch fn u.who args)
::
++  parse-call
  |=  jon=json
  ^-  [fn=@t as=(unit @p) args=json]
  ?>  ?=(%o -.jon)
  =/  fn=@t  (so:dejs:format (~(got by p.jon) 'fn'))
  =/  as=(unit @p)
    ?~  j=(~(get by p.jon) 'as')  ~
    `(slav %p (so:dejs:format u.j))
  =/  args=json  (fall (~(get by p.jon) 'args') ~)
  [fn as args]
::
::  Owner session may act as an %agent member (running a worker).
::  A member/owner `as` is only allowed when it is our (their own login).
::
++  actor-gate
  |=  [m=member:loc as=(unit @p)]
  ^-  (unit @t)
  ?~  as  ~
  ?:  =(who.m our.bowl)  ~
  ?:  =(%agent role.m)  ~
  `'only the owner session may act as an agent; humans log in as themselves'
::
++  dispatch
  |=  [fn=@t actor=member:loc args=json]
  ^-  [json _cor]
  ?:  |(=(fn 'history.purge') =(fn 'zfs.destroy'))
    =.  cor  (note actor fn args 'rejected: immutable')
    [(err:enjs:locus-json 'denied' 'this function cannot empty history') cor]
  ?:  =(fn 'household.list')
    [(ok:enjs:locus-json [members+(members:enjs:locus-json members)]~) cor]
  ?:  =(fn 'household.add')
    (household-add actor args)
  ?:  =(fn 'household.grant')
    (household-grant actor args)
  ?:  =(fn 'household.issue-key')
    (household-issue-key actor args)
  ?:  =(fn 'os.rebuild')
    (queue-os actor fn args)
  ?:  =(fn 'os.rollback')
    (queue-os actor fn args)
  ?:  =(fn 'os.approve')
    (os-decide actor args %approved)
  ?:  =(fn 'os.deny')
    (os-decide actor args %denied)
  ?:  =(fn 'os.executed')
    (os-executed actor args)
  ?:  =(fn 'audit.list')
    [(ok:enjs:locus-json [audit+(audits:enjs:locus-json log)]~) cor]
  =.  cor  (note actor fn args 'rejected: unknown')
  [(err:enjs:locus-json 'unknown-function' fn) cor]
::
++  note
  |=  [actor=member:loc fn=@t args=json result=@t]
  ^+  cor
  =/  id=@ud  +((lent log))
  =/  a=audit:loc  [id now.bowl who.actor fn args result]
  cor(log [a log])
::
++  household-add
  |=  [actor=member:loc args=json]
  ^-  [json _cor]
  ?.  =(%owner role.actor)
    =.  cor  (note actor 'household.add' args 'rejected: owner-only')
    [(err:enjs:locus-json 'not-authorized' 'only owner can add members') cor]
  =/  parsed=(each [who=@p name=@t =role:loc] tang)
    (mule |.((parse-member args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:locus-json 'bad-request' 'need ship, name, role') cor]
  =/  [who=@p name=@t =role:loc]  p.parsed
  ?:  (~(has by members) who)
    [(err:enjs:locus-json 'exists' 'member already present') cor]
  =/  key=@t  (mint who)
  =.  members  (~(put by members) who [who name role])
  =.  keys  (~(put by keys) key who)
  =.  cor  (note actor 'household.add' args 'ok')
  :-  %-  ok:enjs:locus-json
      :~  member+(member:enjs:locus-json [who name role])
          'apiKey'^s+key
      ==
    cor
::
++  household-issue-key
  |=  [actor=member:loc args=json]
  ^-  [json _cor]
  ?.  =(%owner role.actor)
    =.  cor  (note actor 'household.issue-key' args 'rejected: owner-only')
    [(err:enjs:locus-json 'not-authorized' 'only owner can issue keys') cor]
  =/  parsed=(each @p tang)  (mule |.((parse-ship args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:locus-json 'bad-request' 'need ship') cor]
  =/  who=@p  p.parsed
  ?~  m=(~(get by members) who)
    [(err:enjs:locus-json 'unknown-principal' 'not a member') cor]
  =.  keys  (drop-keys who)
  =/  key=@t  (mint who)
  =.  keys  (~(put by keys) key who)
  =.  cor  (note actor 'household.issue-key' args 'ok')
  :-  %-  ok:enjs:locus-json
      :~  member+(member:enjs:locus-json u.m)
          'apiKey'^s+key
      ==
    cor
::
++  mint
  |=  who=ship
  ^-  @t
  (crip (weld (trip (scot %uv eny.bowl)) (trip (scot %p who))))
::
++  drop-keys
  |=  who=ship
  ^-  (map @t ship)
  %-  malt
  %+  skip  ~(tap by keys)
  |=  [k=@t s=ship]
  =(s who)
::
++  parse-ship
  |=  args=json
  ^-  @p
  ?>  ?=(%o -.args)
  (slav %p (so:dejs:format (~(got by p.args) 'ship')))
::
++  household-grant
  |=  [actor=member:loc args=json]
  ^-  [json _cor]
  ?.  =(%owner role.actor)
    =.  cor  (note actor 'household.grant' args 'rejected: owner-only')
    [(err:enjs:locus-json 'not-authorized' 'only owner can change grants') cor]
  =/  parsed=(each [who=@p =role:loc] tang)
    (mule |.((parse-grant args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:locus-json 'bad-request' 'need ship, role') cor]
  =/  [who=@p =role:loc]  p.parsed
  ?~  old=(~(get by members) who)
    [(err:enjs:locus-json 'unknown-principal' 'not a member') cor]
  ?:  &(=(who our.bowl) !=(%owner role))
    [(err:enjs:locus-json 'denied' 'cannot demote the ship owner') cor]
  =/  m=member:loc  u.old(role role)
  =.  members  (~(put by members) who m)
  =.  cor  (note actor 'household.grant' args 'ok')
  [(ok:enjs:locus-json [member+(member:enjs:locus-json m)]~) cor]
::
++  parse-member
  |=  args=json
  ^-  [who=@p name=@t =role:loc]
  ?>  ?=(%o -.args)
  :+  (slav %p (so:dejs:format (~(got by p.args) 'ship')))
    (so:dejs:format (~(got by p.args) 'name'))
  (role:dejs:locus-json (~(got by p.args) 'role'))
::
++  parse-grant
  |=  args=json
  ^-  [who=@p =role:loc]
  ?>  ?=(%o -.args)
  :-  (slav %p (so:dejs:format (~(got by p.args) 'ship')))
  (role:dejs:locus-json (~(got by p.args) 'role'))
::
++  queue-os
  |=  [actor=member:loc fn=@t args=json]
  ^-  [json _cor]
  =/  id=@ud  next-id
  =.  next-id  +(next-id)
  =/  j=job:loc
    [id now.bowl who.actor fn args %pending ~ 'queued']
  =.  jobs  (~(put by jobs) id j)
  =.  cor  (note actor fn args 'queued')
  [(ok:enjs:locus-json [job+(job:enjs:locus-json j)]~) cor]
::
++  parse-id
  |=  args=json
  ^-  @ud
  ?>  ?=(%o -.args)
  (ni:dejs:format (~(got by p.args) 'id'))
::
++  os-decide
  |=  [actor=member:loc args=json want=?(%approved %denied)]
  ^-  [json _cor]
  ?:  =(%agent role.actor)
    =.  cor  (note actor 'os.approve' args 'rejected: agent-cannot-approve')
    [(err:enjs:locus-json 'not-authorized' 'an agent cannot approve OS mutations') cor]
  =/  parsed=(each @ud tang)  (mule |.((parse-id args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:locus-json 'bad-request' 'need numeric id') cor]
  =/  id=@ud  p.parsed
  ?~  job=(~(get by jobs) id)
    [(err:enjs:locus-json 'not-found' 'no such job') cor]
  ?.  =(%pending status.u.job)
    [(err:enjs:locus-json 'conflict' 'job is not pending') cor]
  =/  j=job:loc
    %=  u.job
      status       want
      approved-by  `who.actor
      result       ?:(=(%approved want) 'approved' 'denied')
    ==
  =.  jobs  (~(put by jobs) id j)
  =/  fn=@t  ?:(=(%approved want) 'os.approve' 'os.deny')
  =.  cor  (note actor fn args result.j)
  [(ok:enjs:locus-json [job+(job:enjs:locus-json j)]~) cor]
::
++  os-executed
  |=  [actor=member:loc args=json]
  ^-  [json _cor]
  ?:  =(%agent role.actor)
    =.  cor  (note actor 'os.executed' args 'rejected: agent-cannot-execute')
    [(err:enjs:locus-json 'not-authorized' 'an agent cannot mark OS commands executed') cor]
  =/  parsed=(each @ud tang)  (mule |.((parse-id args)))
  ?:  ?=(%| -.parsed)
    [(err:enjs:locus-json 'bad-request' 'need numeric id') cor]
  =/  id=@ud  p.parsed
  ?~  job=(~(get by jobs) id)
    [(err:enjs:locus-json 'not-found' 'no such job') cor]
  ?.  =(%approved status.u.job)
    [(err:enjs:locus-json 'conflict' 'job is not approved') cor]
  =/  j=job:loc  u.job(status %executed, result 'executed')
  =.  jobs  (~(put by jobs) id j)
  =.  cor  (note actor 'os.executed' args 'executed')
  [(ok:enjs:locus-json [job+(job:enjs:locus-json j)]~) cor]
--
