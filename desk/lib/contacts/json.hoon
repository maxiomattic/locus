/-  con=contacts
|%
++  status-of
  |=  jon=json
  ^-  @ud
  ?.  ?=(%o -.jon)  200
  ?~  e=(~(get by p.jon) 'error')  200
  =/  typ=@t  (so:dejs:format u.e)
  ?+  typ  200
    %bad-request  400
    %not-found    404
    %conflict     409
  ==
::
++  enjs
  =,  enjs:format
  |%
  ++  contact
    |=  c=contact:con
    ^-  json
    %-  pairs
    :~  id+(numb id.c)
        name+s+name.c
        email+s+email.c
        ship+s+ship.c
        revision+(numb revision.c)
    ==
  ::
  ++  contacts
    |=  m=(map @ud contact:con)
    ^-  json
    a+(turn ~(val by m) contact)
  ::
  ++  ok
    |=  extra=(list [@t json])
    ^-  json
    (pairs [[%ok b+&] extra])
  ::
  ++  err
    |=  [typ=@t msg=@t]
    ^-  json
    %-  pairs
    :~  ok+b+|
        error+s+typ
        message+s+msg
    ==
  --
--
