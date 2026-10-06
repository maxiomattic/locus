::  P2 mail store — create, list, expectedRevision conflict, ingest.
::
/-  mail
/+  *test-agent
/=  mail-agent  /app/mail
|%
++  dap  %mail
++  init
  =/  m  (mare ,~)
  ^-  form:m
  ;<  *  bind:m  (do-init dap mail-agent)
  (pure:m ~)
::
++  call
  |=  jon=json
  (do-poke %json !>(jon))
::
++  create
  %-  pairs:enjs:format
  :~  fn+s+'message.create'
      args+(pairs:enjs:format ~[subject+s+'hello' body+s+'draft body'])
  ==
::
++  patch
  |=  [id=@ud rev=@ud body=@t]
  %-  pairs:enjs:format
  :~  fn+s+'message.patch'
      args+(pairs:enjs:format ~[id+(numb:enjs:format id) 'expectedRevision'^(numb:enjs:format rev) body+s+body])
  ==
::
++  ingest
  %-  pairs:enjs:format
  :~  fn+s+'ingest'
      args+(pairs:enjs:format ~[from+s+'a@b.com' to+s+'c@d.com' subject+s+'in' body+s+'hi'])
  ==
::
++  send
  |=  id=@ud
  %-  pairs:enjs:format
  :~  fn+s+'send'
      args+(pairs:enjs:format ~[id+(numb:enjs:format id)])
  ==
::
++  test-create-and-list
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call create)
  ;<  =cage  bind:m  (got-peek /x/messages)
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
  ;<  =cage  bind:m  (got-peek /x/messages)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  ?>  ?=(%o -.i.p.jon)
  =/  rev=json  (~(got by p.i.p.jon) 'revision')
  ?>  ?=(%n -.rev)
  ?>  =('1' p.rev)
  (pure:m ~)
::
++  test-ingest-inbox-and-send
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call ingest)
  ;<  *  bind:m  (call create)
  ;<  *  bind:m  (call (send 2))
  ;<  =cage  bind:m  (got-peek /x/messages)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ^ ~] p.jon)
  (pure:m ~)
--
