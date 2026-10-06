::  P2 calendar store — create, list, expectedRevision conflict.
::
/-  cal=calendar
/+  *test-agent
/=  calendar-agent  /app/calendar
|%
++  dap  %calendar
++  init
  =/  m  (mare ,~)
  ^-  form:m
  ;<  *  bind:m  (do-init dap calendar-agent)
  (pure:m ~)
::
++  call
  |=  jon=json
  (do-poke %json !>(jon))
::
++  create
  %-  pairs:enjs:format
  :~  fn+s+'event.create'
      args+(pairs:enjs:format ~[title+s+'standup' start+(numb:enjs:format 1.700.000.000) end+(numb:enjs:format 1.700.003.600)])
  ==
::
++  patch
  |=  [id=@ud rev=@ud title=@t]
  %-  pairs:enjs:format
  :~  fn+s+'event.patch'
      args+(pairs:enjs:format ~[id+(numb:enjs:format id) 'expectedRevision'^(numb:enjs:format rev) title+s+title])
  ==
::
++  test-create-and-list
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call create)
  ;<  =cage  bind:m  (got-peek /x/events)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  (pure:m ~)
::
++  test-stale-patch-keeps-revision
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call create)
  ;<  *  bind:m  (call (patch 1 0 'ok'))
  ;<  *  bind:m  (call (patch 1 0 'stale'))
  ;<  =cage  bind:m  (got-peek /x/events)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  ?>  ?=(%o -.i.p.jon)
  =/  rev=json  (~(got by p.i.p.jon) 'revision')
  ?>  ?=(%n -.rev)
  ?>  =('1' p.rev)
  (pure:m ~)
--
