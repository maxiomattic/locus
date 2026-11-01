::  %store as an agent: batches, proposals, approval, undo and tools.
::
/-  *store
/+  *test-agent, *store
/=  store-agent  /app/store
|%
++  dap  %store
++  init
  =/  m  (mare ,~)
  ^-  form:m
  ;<  *  bind:m  (do-init dap store-agent)
  (pure:m ~)
++  j  |=(t=@t (need (de:json:html t)))
++  act  |=(t=@t (do-poke %json !>((j t))))
++  show
  |=  id=@t
  =/  m  (mare ,json)
  ^-  form:m
  ;<  =cage  bind:m  (got-peek /x/object/[id])
  (pure:m !<(json q.cage))
++  field
  |=  [o=json k=@t]
  ^-  json
  ?>  ?=([%o *] o)
  =/  f  (~(got by p.o) 'fields')
  ?>  ?=([%o *] f)
  (~(gut by p.f) k ~)
::
++  test-batch-applies
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (act '{"do":"batch","intent":"new","ops":[{"do":"make","id":"a","type":"note","fields":{"title":"x"}}]}')
  ;<  o=json  bind:m  (show 'a')
  (ex-equal !>((field o 'title')) !>(`json`s+'x'))
::
++  test-proposal-then-approve
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (act '{"do":"propose","actor":"agent:a","intent":"tag","ops":[{"do":"make","id":"a","type":"note","fields":{"title":"x"}}]}')
  ;<  q=cage  bind:m  (got-peek /x/state)
  =/  s=json  !<(json q.q)
  ?>  ?=([%o *] s)
  =/  os  (~(got by p.s) 'objects')
  ?>  ?=([%a [%o *] ~] os)
  =/  pid=json  (~(got by p.i.p.os) 'id')
  ?>  ?=([%s *] pid)
  ;<  *  bind:m  (do-poke %json !>(`json`(pairs:enjs:format ~[do+s+'approve' id+pid])))
  ;<  o=json  bind:m  (show 'a')
  ;<  q2=json  bind:m  (show p.pid)
  ;<  ~  bind:m  (ex-equal !>((field o 'title')) !>(`json`s+'x'))
  (ex-equal !>((field q2 'status')) !>(`json`s+'applied'))
--
