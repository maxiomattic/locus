::  P4 contacts — create, list, expectedRevision conflict.
::
/-  con=contacts
/+  *test-agent
/=  contacts-agent  /app/contacts
|%
++  dap  %contacts
++  init
  =/  m  (mare ,~)
  ^-  form:m
  ;<  *  bind:m  (do-init dap contacts-agent)
  (pure:m ~)
::
++  call
  |=  jon=json
  (do-poke %json !>(jon))
::
++  create
  %-  pairs:enjs:format
  :~  fn+s+'contact.create'
      args+(pairs:enjs:format ~[name+s+'Ada' email+s+'ada@home' ship+s+'~zod'])
  ==
::
++  patch
  |=  [id=@ud rev=@ud name=@t]
  %-  pairs:enjs:format
  :~  fn+s+'contact.patch'
      args+(pairs:enjs:format ~[id+(numb:enjs:format id) 'expectedRevision'^(numb:enjs:format rev) name+s+name])
  ==
::
++  test-create-and-list
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call create)
  ;<  =cage  bind:m  (got-peek /x/contacts)
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
  ;<  =cage  bind:m  (got-peek /x/contacts)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  ?>  ?=(%o -.i.p.jon)
  =/  rev=json  (~(got by p.i.p.jon) 'revision')
  ?>  ?=(%n -.rev)
  ?>  =('1' p.rev)
  (pure:m ~)
--
