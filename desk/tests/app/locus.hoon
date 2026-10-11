::  Clan command plane — identity, OS approval, immutable audit.
::
/-  loc=locus
/+  *test-agent
/=  locus-agent  /app/locus
|%
++  dap  %locus
++  init
  =/  m  (mare ,~)
  ^-  form:m
  ;<  *  bind:m  (do-init dap locus-agent)
  (pure:m ~)
::
++  call
  |=  jon=json
  (do-poke %json !>(jon))
::
++  add-agent
  %-  pairs:enjs:format
  :~  fn+s+'household.add'
      args+(pairs:enjs:format ~[ship+s+'~dev' name+s+'worker' role+s+'agent'])
  ==
::
++  rebuild-as-dev
  %-  pairs:enjs:format
  :~  fn+s+'os.rebuild'
      as+s+'~dev'
      args+(pairs:enjs:format ~[flake+s+'.#server'])
  ==
::
++  approve-as-dev
  |=  id=@ud
  %-  pairs:enjs:format
  :~  fn+s+'os.approve'
      as+s+'~dev'
      args+(pairs:enjs:format ~[id+(numb:enjs:format id)])
  ==
::
++  approve
  |=  id=@ud
  %-  pairs:enjs:format
  :~  fn+s+'os.approve'
      args+(pairs:enjs:format ~[id+(numb:enjs:format id)])
  ==
::
++  purge
  %-  pairs:enjs:format
  :~  fn+s+'history.purge'
      args+~
  ==
::
++  test-owner-on-init
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  =bowl:gall  bind:m  get-bowl
  ;<  =cage  bind:m  (got-peek /x/members)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  (pure:m ~)
::
++  test-add-agent-and-list
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call add-agent)
  ;<  =cage  bind:m  (got-peek /x/members)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ^ ~] p.jon)
  ;<  =cage  bind:m  (got-peek (weld /x/has-key [(scot %p ~dev)]~))
  =/  has=json  !<(json q.cage)
  ?>  ?=(%b -.has)
  ?>  p.has
  (pure:m ~)
::
++  test-agent-rebuild-queues
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call add-agent)
  ;<  *  bind:m  (call rebuild-as-dev)
  ;<  =cage  bind:m  (got-peek /x/pending)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  (pure:m ~)
::
++  test-agent-cannot-approve
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call add-agent)
  ;<  *  bind:m  (call rebuild-as-dev)
  ;<  *  bind:m  (call (approve-as-dev 1))
  ;<  =cage  bind:m  (got-peek /x/pending)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  (pure:m ~)
::
++  test-owner-approve-clears-pending
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call add-agent)
  ;<  *  bind:m  (call rebuild-as-dev)
  ;<  *  bind:m  (call (approve 1))
  ;<  =cage  bind:m  (got-peek /x/pending)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=(~ p.jon)
  (pure:m ~)
::
++  test-purge-denied-audit-remains
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call add-agent)
  ;<  *  bind:m  (call purge)
  ;<  =cage  bind:m  (got-peek /x/audit)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=(^ p.jon)
  (pure:m ~)
--
