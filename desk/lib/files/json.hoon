/-  fil=files
|%
::  HTTP status for a json reply: errors carry an error type, ok replies are 200.
::
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
  ++  file
    |=  f=file:fil
    ^-  json
    %-  pairs
    :~  id+(numb id.f)
        dir+s+dir.f
        name+s+name.f
        sha256+s+sha256.f
        size+(numb size.f)
        mime+s+mime.f
        created+(numb (unt:chrono:userlib created.f))
        modified+(numb (unt:chrono:userlib modified.f))
        revision+(numb revision.f)
    ==
  ::
  ++  files
    |=  l=(list file:fil)
    ^-  json
    a+(turn l file)
  ::
  ++  dirs
    |=  l=(list @t)
    ^-  json
    a+(turn l |=(d=@t s+d))
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
