::  %store: the clan's objects. Everything is built from six primitives:
::  Object, Type, Operation, Reference, Blob and View. Types, views, tools,
::  grants and members are themselves objects, so changing one is an
::  audited operation like any other.
::
|%
+$  oid  @t
+$  fields  (map @t json)
::  A field's clock is the object revision that last wrote it, so a write
::  based on an older revision can be seen to conflict with that field only.
::
+$  object
  $:  id=oid
      type=@t
      rev=@ud
      =fields
      clock=(map @t @ud)
      trashed=?
      clashes=(list clash)
      by=@t
      at=@da
  ==
::  A write that lost to a newer one. It is kept, never dropped.
::
+$  clash  [field=@t theirs=json actor=@t seq=@ud]
::  The only ways anything changes. A null field value removes the field.
::  base 0 means the writer does not care what came before.
::
+$  op
  $%  [%make id=oid type=@t =fields]
      [%set id=oid base=@ud =fields]
      [%splice id=oid field=@t del=(list json) add=(list [after=json val=json])]
      [%trash id=oid]
      [%restore id=oid]
      [%resolve id=oid field=@t keep=?]
      [%purge id=oid]
  ==
::  One atomic change. cid lets a client match its optimistic copy to the
::  home ship's order.
::
+$  batch  [cid=@t actor=@t intent=@t ops=(list op)]
+$  entry
  $:  seq=@ud
      at=@da
      who=ship
      actor=@t
      intent=@t
      cid=@t
      ops=(list op)
      undo=(list op)
  ==
::  Ship-to-ship traffic. A client pokes %push; home answers on /log/<from>.
::
+$  wire-poke
  $%  [%push =batch]
      [%decide id=oid ok=?]
  ==
+$  wire-fact
  $%  [%snap seq=@ud objects=(map oid object)]
      [%entry =entry]
  ==
+$  log  ((mop @ud entry) lth)
+$  state-0
  $:  %0
      objects=(map oid object)                ::  what readers see
      base=(map oid object)                   ::  client: confirmed by home
      =log
      seq=@ud
      upstream=(unit ship)                    ::  ~ means this ship is home
      pending=(list batch)                    ::  client: applied, not confirmed
      rejected=(list [cid=@t why=@t])
      members=(set ship)
      reacted=(map oid @uv)                   ::  a reaction's last view result
      ver=@ud                                 ::  bumps on any visible change
      touched=(map oid @ud)                   ::  ver at which an object changed
  ==
--
