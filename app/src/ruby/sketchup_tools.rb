# The interactive and reading side of SketchUp's Ruby API, on top of sketchup.rb:
# the model as it was when the script started (read-only copy), Sketchup::Tool and
# its mouse/keyboard events, the view and input points, observers, UI::HtmlDialog,
# timers, Follow Me and textures.
#
# Nothing here reaches outside the sandbox. A tool is run by feeding it events (the
# app sends mouse and key events as they happen; tests send a list): the script is
# loaded, the tool activated, each event delivered, and what the tool did is the
# journal, as for any other script. Dialogs are recorded (their HTML, what the script
# sent them) and their callbacks are called by events.

module SC
  KEYS = {}

  # the model's faces as they were when the run started (host sends them as meters)
  def self.seed_model(model)
    snap = `globalThis.__model`
    return if `#{snap} == null`
    in_ = 1.0 / M_PER_IN
    verts = {}
    edges = {}
    etarget = {}
    groups = {}
    (`#{snap}.groups || []`).each do |g|
      id = `#{g}.id`
      groups[id] = Sketchup::Selected.new(id, `#{g}.name || ""`, `#{g}.kind || "group"`)
    end
    index = {}
    (`#{snap}.faces || []`).each do |f|
      id = `#{f}.id`
      gid = `#{f}.group`
      gid = nil if `#{gid} == null`
      target = gid && groups[gid] ? groups[gid].entities : model.entities
      outer = (`#{f}.outer || []`).map { |p| Geom::Point3d.new(`#{p}[0]` * in_, `#{p}[1]` * in_, `#{p}[2]` * in_) }
      next if outer.length < 3
      nrm = `#{f}.normal`
      face = Sketchup::Face.new(id.to_s, outer, Geom::Vector3d.new(`#{nrm}[0]`, `#{nrm}[1]`, `#{nrm}[2]`))
      face.instance_variable_set(:@real, true)
      face.instance_variable_set(:@material, `#{f}.material`)
      inner = (`#{f}.inner || []`).map { |h| h.map { |p| Geom::Point3d.new(`#{p}[0]` * in_, `#{p}[1]` * in_, `#{p}[2]` * in_) } }
      face.instance_variable_set(:@inner, inner)
      vs = outer.map { |p| verts[SC.key(p)] ||= Sketchup::Vertex.new(p) }
      es = outer.each_index.map do |i|
        a = vs[i]
        b = vs[(i + 1) % vs.length]
        k = [SC.key(a.position), SC.key(b.position)].sort.join("|")
        e = (edges[k] ||= Sketchup::Edge.new(nil, a.position, b.position).tap { |x| x.link(a, b); etarget[x] = target })
        e.add_face(face)
        e
      end
      face.link(vs, es)
      index[id] = face
      target.keep(face)
    end
    edges.each_value { |e| etarget[e].keep(e) }
    @snapshot_faces = index
    @snapshot_groups = groups
    sel = `globalThis.__selection || []`
    items = sel.map do |s|
      id = `#{s}.id`
      index[id] || groups[id] || Sketchup::Selected.new(id, `#{s}.name || ""`, `#{s}.kind`)
    end
    model.selection.replace_silently(items)
  end

  def self.key(p); "#{(p.x * 1000).round},#{(p.y * 1000).round},#{(p.z * 1000).round}"; end

  # tell each observer (that has the method) what happened
  def self.notify(list, meth, *args)
    list.dup.each { |o| o.__send__(meth, *args) if o.respond_to?(meth) }
  end

  @status = nil
  def self.status=(t); @status = t; `globalThis.__status = #{t.to_s}`; end

  def self.view; @view ||= Sketchup::View.new; end
  def self.tool; @tool; end
  def self.tool=(t); @tool = t; end
  def self.dialogs; @dialogs ||= []; end
  def self.timers; @timers ||= {}; end
  def self.defaults; @defaults ||= {}; end

  def self.at(ev)
    a = `#{ev}.at`
    return nil if `#{a} == null`
    Geom::Point3d.new(`#{a}[0]` / M_PER_IN, `#{a}[1]` / M_PER_IN, `#{a}[2]` / M_PER_IN)
  end

  # deliver the app's events (mouse, keys, selection, dialog calls, timers) to what the script set up
  def self.dispatch(events)
    events.each do |ev|
      t = `#{ev}.type`.to_s
      v = view
      v.point = SC.at(ev) if SC.at(ev)
      x = (`#{ev}.x` || 0).to_i
      y = (`#{ev}.y` || 0).to_i
      flags = (`#{ev}.flags` || 0).to_i
      v.cursor = [x, y]
      tl = @tool
      case t
      when "mouse_move" then tl.onMouseMove(flags, x, y, v) if tl && tl.respond_to?(:onMouseMove)
      when "lbutton_down" then tl.onLButtonDown(flags, x, y, v) if tl && tl.respond_to?(:onLButtonDown)
      when "lbutton_up" then tl.onLButtonUp(flags, x, y, v) if tl && tl.respond_to?(:onLButtonUp)
      when "double_click" then tl.onLButtonDoubleClick(flags, x, y, v) if tl && tl.respond_to?(:onLButtonDoubleClick)
      when "rbutton_down" then tl.onRButtonDown(flags, x, y, v) if tl && tl.respond_to?(:onRButtonDown)
      when "rbutton_up" then tl.onRButtonUp(flags, x, y, v) if tl && tl.respond_to?(:onRButtonUp)
      when "key_down" then tl.onKeyDown(`#{ev}.key`.to_i, (`#{ev}.repeat` || 1).to_i, flags, v) if tl && tl.respond_to?(:onKeyDown)
      when "key_up" then tl.onKeyUp(`#{ev}.key`.to_i, 1, flags, v) if tl && tl.respond_to?(:onKeyUp)
      when "return" then tl.onReturn(v) if tl && tl.respond_to?(:onReturn)
      when "cancel" then tl.onCancel((`#{ev}.reason` || 0).to_i, v) if tl && tl.respond_to?(:onCancel)
      when "user_text" then tl.onUserText(`#{ev}.text`.to_s, v) if tl && tl.respond_to?(:onUserText)
      when "suspend" then tl.suspend(v) if tl && tl.respond_to?(:suspend)
      when "resume" then tl.resume(v) if tl && tl.respond_to?(:resume)
      when "selection"
        ids = (`#{ev}.ids` || [])
        items = ids.map { |i| @snapshot_faces && @snapshot_faces[i] || @snapshot_groups && @snapshot_groups[i] || Sketchup::Selected.new(i, "", "face") }
        Sketchup.active_model.selection.replace(items)
      when "dialog"
        d = dialogs[`#{ev}.id`.to_i]
        raise "no dialog #{`#{ev}.id`}" unless d
        d.call_back(`#{ev}.name`.to_s, `#{ev}.params`.to_s)
      when "tick"
        timers.values.each { |tm| tm.fire }
      when "undo" then SC.notify(Sketchup.active_model.observers, :onTransactionUndo, Sketchup.active_model)
      else
        raise "unknown event #{t}"
      end
      tl.draw(v) if tl && tl.respond_to?(:draw)
    end
  end

  # leave the tool when the run ends, as SketchUp does when another tool is picked
  def self.end_tool
    return unless @tool
    t = @tool
    @tool = nil
    t.deactivate(view) if t.respond_to?(:deactivate)
  end

  # ---- Follow Me and textures --------------------------------------------

  # the points of a path given as edges / a curve / points, in order
  def self.path_points(path)
    list = path.flatten.compact
    return list.map { |p| SC.pt(p) } if list.all? { |p| p.is_a?(Array) || p.is_a?(Geom::Point3d) }
    edges = list.flat_map { |e| e.respond_to?(:edges) && !e.is_a?(Sketchup::Edge) ? e.edges : [e] }
    raise ArgumentError, "Follow Me needs a path of edges or points" unless edges.all? { |e| e.is_a?(Sketchup::Edge) }
    pts = [SC.pt(edges[0].start.position)]
    edges.each do |e|
      a = SC.pt(e.start.position)
      b = SC.pt(e.end.position)
      last = pts.last
      d = ->(p, q) { (p[0] - q[0])**2 + (p[1] - q[1])**2 + (p[2] - q[2])**2 }
      if d.(last, a) < 1e-6 then pts << b
      elsif d.(last, b) < 1e-6 then pts << a
      else
        # edges in the wrong order or direction: chain from whichever end meets
        pts = [b, a] + pts if d.(pts.first, a) < 1e-6 || d.(pts.first, b) < 1e-6
        pts << b if pts.last != b && pts.last == last
      end
    end
    pts
  end
end

module Geom
  def self.intersect_line_plane(line, plane)
    pt, v = line
    pt = Point3d.new(pt)
    v = Vector3d.new(v.respond_to?(:to_a) ? v.to_a : v)
    if plane.length == 2
      ppt = Point3d.new(plane[0])
      n = Vector3d.new(plane[1])
      d = -(n.x * ppt.x + n.y * ppt.y + n.z * ppt.z)
    else
      n = Vector3d.new(plane[0], plane[1], plane[2])
      d = plane[3]
    end
    den = n.dot(v)
    return nil if den.abs < 1e-12
    t = -(n.x * pt.x + n.y * pt.y + n.z * pt.z + d) / den
    Point3d.new(pt.x + v.x * t, pt.y + v.y * t, pt.z + v.z * t)
  end

  def self.point_in_polygon_2D(pt, poly, check_border = true)
    x, y = pt[0], pt[1]
    inside = false
    j = poly.length - 1
    poly.each_with_index do |p, i|
      q = poly[j]
      if (p[1] > y) != (q[1] > y) && x < (q[0] - p[0]) * (y - p[1]) / (q[1] - p[1]).to_f + p[0]
        inside = !inside
      end
      j = i
    end
    inside
  end
end

module Sketchup
  # ---- the model as it was: edges, vertices, faces know each other ----------

  class Vertex
    def edges; (@edges ||= []); end
    def faces; edges.flat_map(&:faces).uniq; end
    def link_edge(e); (@edges ||= []) << e unless (@edges ||= []).include?(e); end
    def ==(o); o.is_a?(Vertex) && position == o.position; end
  end

  class Edge
    def link(a, b); @verts = [a, b]; a.link_edge(self); b.link_edge(self); end
    def add_face(f); (@faces ||= []) << f unless (@faces ||= []).include?(f); end
    def faces; @faces ||= []; end
    def start; @verts ? @verts[0] : Vertex.new(@points[0]); end
    def end; @verts ? @verts[1] : Vertex.new(@points[1]); end
    def other_vertex(v); v == start ? self.end : start; end
    def used_by?(f); faces.include?(f); end
    def soft?; !!@soft; end
    def smooth?; !!@smooth; end
    def curve; nil; end
    def common_face(o); (faces & o.faces).first; end
    def reversed_in?(f); false; end
    def typename; "Edge"; end
    def erase!; SC.unsupported("Edge#erase! (an edge of existing geometry)") unless @ref; super; end
  end

  class Face
    def link(vs, es); @vert_objs = vs; @edge_objs = es; end
    def vertices; @vert_objs || @outer.map { |p| Vertex.new(p) }; end
    def edges; @edge_objs || (0...@outer.length).map { |i| Edge.new(nil, @outer[i], @outer[(i + 1) % @outer.length]) }; end
    def loops; [LoopOf.new(self, edges, vertices)] + (@inner || []).map { |h| LoopOf.new(self, [], h.map { |p| Vertex.new(p) }) }; end
    def outer_loop; loops[0]; end
    def typename; "Face"; end
    def parent; Sketchup.active_model; end
    def all_connected; (SC.snapshot_connected(self)); end
    def classify_point(p)
      a = Geom::Point3d.new(p)
      Geom.point_in_polygon_2D(a, @outer) ? 1 : 16
    end
    # sweep this face along a path of edges / a curve / points (Follow Me)
    def followme(*path)
      pts = SC.path_points(path)
      raise ArgumentError, "Follow Me needs a path of at least two points" if pts.length < 2
      SC.emit({ cmd: "follow_me", face: jid, points: pts.map { |p| SC.m(p) } })
      true
    end
  end

  class LoopOf
    attr_reader :face, :edges, :vertices
    def initialize(face, edges, vertices); @face = face; @edges = edges; @vertices = vertices; end
    def outer?; @face.outer.length == @vertices.length && @face.vertices.zip(@vertices).all? { |a, b| a.equal?(b) || a.position == b.position }; end
    def edgeuses; @edges; end
  end

  def self.snapshot_connected(f); [f]; end

  class Selected
    def vertices; []; end
    def edges; []; end
  end

  # ---- textures ----------------------------------------------------------------

  class Texture
    attr_reader :filename, :data
    attr_accessor :width, :height
    def initialize(data, w, h); @data = data; @filename = data.start_with?("data:") ? "texture.png" : data; @width = w; @height = h; end
    def image_width; 128; end
    def image_height; 128; end
    def size=(s); @width = s.to_f; @height = s.to_f; end
    def valid?; true; end
  end

  class Material
    def texture; @texture; end
    # a texture is an image the script can't read (no files): pass a data: URL,
    # or [data_url, width, height] (inches, as in SketchUp)
    def texture=(src)
      return (@texture = nil) if src.nil?
      src, w, h = src if src.is_a?(Array)
      raise ArgumentError, "Material#texture= needs an image as a data: URL (a script can't read files in SketchCraft)" unless src.is_a?(String) && src.start_with?("data:")
      @texture = Texture.new(src, (w || 12.0).to_f, (h || w || 12.0).to_f)
      SC.emit({ cmd: "texture", name: @name, data: src, width: @texture.width * SC::M_PER_IN, height: @texture.height * SC::M_PER_IN })
    end
    def use_alpha?; false; end
  end

  # ---- selection (changes tell its observers) ---------------------------------

  class Selection
    def observers; @observers ||= []; end
    def add_observer(o); observers << o; true; end
    def remove_observer(o); observers.delete(o); true; end
    def replace_silently(items); @items = items.dup; end
    def replace(items)
      return if items.map { |i| i.entityID } == @items.map { |i| i.entityID }
      @items = items.dup
      SC.notify(observers, :onSelectionBulkChange, self)
    end
    def clear
      return self if @items.empty?
      @items = []
      SC.notify(observers, :onSelectionCleared, self)
      SC.notify(observers, :onSelectionBulkChange, self)
      self
    end
    def add(*ents)
      ents.flatten.each { |e| @items << e unless contains?(e) }
      SC.notify(observers, :onSelectionBulkChange, self)
      @items.length
    end
    def remove(*ents)
      ents.flatten.each { |e| @items.delete_if { |i| i.equal?(e) || i.entityID == e.entityID } }
      SC.notify(observers, :onSelectionBulkChange, self)
    end
    def toggle(*ents); ents.flatten.each { |e| contains?(e) ? @items.delete(e) : @items << e }; SC.notify(observers, :onSelectionBulkChange, self); end
    def contains?(e); @items.any? { |i| i.equal?(e) || i.entityID == e.entityID }; end
    alias include? contains?
    def model; Sketchup.active_model; end
    def single_object?; @items.length == 1; end
    def grep(k, &b); @items.grep(k, &b); end
    def last; @items.last; end
  end

  # ---- observers -------------------------------------------------------------

  %w[SelectionObserver EntitiesObserver ModelObserver EntityObserver InstanceObserver ToolsObserver
     AppObserver MaterialsObserver LayersObserver DefinitionsObserver DefinitionObserver ViewObserver
     FrameChangeObserver PagesObserver RenderingOptionsObserver ShadowInfoObserver OptionsProviderObserver
     ComponentObserver].each { |n| const_set(n, Class.new) unless const_defined?(n) }

  class Entity
    def observers; @observers ||= []; end
    def add_observer(o); observers << o; true; end
    def remove_observer(o); observers.delete(o); true; end
  end

  class Entities
    def observers; @observers ||= []; end
    def add_observer(o); observers << o; true; end
    def remove_observer(o); observers.delete(o); true; end
    alias keep_without_observers keep
    def keep(e)
      keep_without_observers(e)
      SC.notify(observers, :onElementAdded, self, e)
      e
    end
    def grep(k, &b); to_a.grep(k, &b); end
    def add_faces_from_mesh(*_); SC.unsupported("Entities#add_faces_from_mesh"); end
  end

  class Model
    def observers; @observers ||= []; end
    def add_observer(o); observers << o; true; end
    def remove_observer(o); observers.delete(o); true; end
    def tools; @tools ||= Tools.new; end
    def active_view; SC.view; end
    def select_tool(t)
      SC.end_tool
      SC.tool = t
      t.activate if t && t.respond_to?(:activate)
      SC.notify(tools.observers, :onActiveToolChanged, tools, t.class.name, 0) if t
      true
    end
    alias start_operation_plain start_operation
    def start_operation(*a); SC.notify(observers, :onTransactionStart, self); true; end
    def commit_operation; SC.notify(observers, :onTransactionCommit, self); true; end
    def definition_list; definitions; end
    def get_attribute(d, k, default = nil); (@attrs ||= {}).fetch(d, {}).fetch(k, default); end
    def set_attribute(d, k, v); ((@attrs ||= {})[d] ||= {})[k] = v; end
    def attribute_dictionary(n, create = false); (@attrs ||= {})[n] ||= {} if create; (@attrs ||= {})[n]; end
    def modified?; false; end
    def valid?; true; end
    def guid; "sketchcraft"; end
    def active_path; nil; end
    def edit_transform; Geom::Transformation.new; end
    def rendering_options; @rendering_options ||= {}; end
    def pages; []; end
    def selection; @selection; end
  end

  class Tools
    def observers; @observers ||= []; end
    def add_observer(o); observers << o; true; end
    def remove_observer(o); observers.delete(o); true; end
    def push_tool(t); @stack = (@stack || []) << SC.tool; Sketchup.active_model.select_tool(t); end
    def pop_tool; Sketchup.active_model.select_tool((@stack || []).pop); end
    def active_tool_id; 0; end
  end

  def self.add_observer(o); (@app_observers ||= []) << o; true; end
  def self.remove_observer(o); (@app_observers ||= []).delete(o); true; end
  def self.active_model_selection; active_model.selection; end
  def self.read_default(section, key, default = nil); SC.defaults.fetch("#{section}/#{key}", default); end
  def self.write_default(section, key, value); SC.defaults["#{section}/#{key}"] = value; end
  def self.is_64bit?; true; end
  def self.get_locale; "en-US"; end
  def self.app_name; "SketchUp"; end
  def self.status_text=(t); SC.status = t; end
  def self.set_status_text(t, *_); SC.status = t; end
  def self.vcb_value=(v); `globalThis.__vcb = #{v.to_s}`; end
  def self.vcb_label=(v); `globalThis.__vcb_label = #{v.to_s}`; end
  def self.undo; SC.unsupported("Sketchup.undo (undo from the Edit menu)"); end
  def self.file_new; SC.unsupported("Sketchup.file_new"); end

  # ---- the tool, the view and what it sees ---------------------------------------

  class Tool
    def activate; end
    def deactivate(view); end
    def suspend(view); end
    def resume(view); end
    def onCancel(reason, view); end
    def onReturn(view); end
    def onUserText(text, view); end
    def onKeyDown(key, repeat, flags, view); end
    def onKeyUp(key, repeat, flags, view); end
    def onLButtonDown(flags, x, y, view); end
    def onLButtonUp(flags, x, y, view); end
    def onLButtonDoubleClick(flags, x, y, view); end
    def onRButtonDown(flags, x, y, view); end
    def onRButtonUp(flags, x, y, view); end
    def onMouseMove(flags, x, y, view); end
    def onSetCursor; 0; end
    def draw(view); end
    def getExtents; Geom::BoundingBox.new; end
    def enableVCB?; false; end
    def getMenu(menu); end
    def statusText; ""; end
  end

  class Camera
    def eye; Geom::Point3d.new(100, -100, 100); end
    def target; Geom::Point3d.new(0, 0, 0); end
    def direction; Geom::Vector3d.new(-1, 1, -1).normalize; end
    def up; Geom::Vector3d.new(0, 0, 1); end
    def perspective?; true; end
    def fov; 35.0; end
    def zaxis; direction; end
    def xaxis; Geom::Vector3d.new(1, 1, 0).normalize; end
    def yaxis; up; end
  end

  class InputPoint
    attr_reader :position
    def initialize(*a); @position = a[0].is_a?(Geom::Point3d) ? a[0] : nil; @valid = !@position.nil?; end
    def pick(view, x, y, ref = nil)
      @position = view.point
      @valid = !@position.nil?
      @valid
    end
    def valid?; @valid; end
    def clear; @position = nil; @valid = false; end
    def copy!(o); @position = o.position; @valid = o.valid?; self; end
    def face; nil; end
    def edge; nil; end
    def vertex; nil; end
    def instance_path; nil; end
    def degrees_of_freedom; 3; end
    def display?; false; end
    def tooltip; ""; end
    def transformation; Geom::Transformation.new; end
    def draw(view); end
    def ==(o); o.is_a?(InputPoint) && position == o.position; end
  end

  class PickHelper
    def do_pick(*_); 0; end
    def count; 0; end
    def all_picked; []; end
    def picked_face; nil; end
    def picked_edge; nil; end
    def best_picked; nil; end
    def init(*_); true; end
    def pick_segment(*_); false; end
  end

  class View
    attr_accessor :point, :cursor
    DRAW_CALLS = %w[draw draw2d draw_line draw_lines draw_points draw_polyline draw_text drawing_color= line_width=
                    line_stipple= set_color_from_line draw_geometry].freeze
    def initialize; @point = nil; @cursor = [0, 0]; @camera = Camera.new; end
    DRAW_CALLS.each { |n| define_method(n) { |*_a| nil } }
    def invalidate; self; end
    def refresh; self; end
    def model; Sketchup.active_model; end
    def camera; @camera; end
    def vpwidth; 1280; end
    def vpheight; 800; end
    def inputpoint(x = nil, y = nil, *_); InputPoint.new(@point); end
    def pick_helper(*_); PickHelper.new; end
    def pickray(*_); [camera.eye, camera.direction]; end
    def screen_coords(pt); Geom::Point3d.new(@cursor[0], @cursor[1], 0); end
    def guess_target_view; self; end
    def tooltip=(t); `globalThis.__tooltip = #{t.to_s}`; end
    def lock_inference(*_); end
    def zoom_extents; self; end
    def center; self; end
    def text_bounds(*_); Geom::BoundingBox.new; end
    def pixels_to_model(px, pt); px * 0.5; end
    def remember_camera; self; end
    def add_observer(o); (@observers ||= []) << o; true; end
    def remove_observer(o); (@observers ||= []).delete(o); true; end
    def method_missing(name, *_); SC.unsupported("View##{name}"); end
    def respond_to_missing?(*_); false; end
  end
end

VK_SHIFT = 16
VK_CONTROL = 17
VK_ALT = 18
VK_MENU = 18
VK_COMMAND = 91
VK_LEFT = 37
VK_UP = 38
VK_RIGHT = 39
VK_DOWN = 40
VK_DELETE = 46
VK_RETURN = 13
VK_ESCAPE = 27
VK_SPACE = 32
VK_TAB = 9
VK_BACK = 8
CONSTRAIN_MODIFIER_KEY = VK_SHIFT
CONSTRAIN_MODIFIER_MASK = 4
COPY_MODIFIER_KEY = VK_CONTROL
COPY_MODIFIER_MASK = 8
ALT_MODIFIER_KEY = VK_ALT
ALT_MODIFIER_MASK = 32
MK_LBUTTON = 1
MK_RBUTTON = 2
MK_SHIFT = 4
MK_CONTROL = 8
MK_ALT = 32
IDCANCEL = 2

# ---- dialogs and timers ----------------------------------------------------------

module UI
  class HtmlDialog
    attr_reader :id, :title
    def initialize(props = {})
      props = {} unless props.is_a?(Hash)
      @title = (props[:dialog_title] || props[:title] || "").to_s
      @callbacks = {}
      @scripts = []
      @html = ""
      @visible = false
      @id = SC.dialogs.length
      SC.dialogs << self
    end
    def set_html(html); @html = html.to_s; end
    def set_url(url); @html = ""; @url = url.to_s; end
    def set_file(path); @html = ""; @file = path.to_s; end
    def add_action_callback(name, &blk); @callbacks[name.to_s] = blk; end
    def execute_script(js); @scripts << js.to_s; end
    def show; @visible = true; self; end
    def show_modal; show; end
    def close; @visible = false; @closed.call if @closed; end
    def visible?; @visible; end
    def bring_to_front; self; end
    def center; self; end
    def set_size(w, h); @size = [w, h]; end
    def set_position(x, y); end
    def set_on_closed(&b); @closed = b; end
    def set_can_close(&b); end
    def set_background_color(c); end
    def set_minimum_size(*_); end
    def set_maximum_size(*_); end
    def write_image(*_); SC.unsupported("HtmlDialog#write_image"); end
    def call_back(name, params)
      cb = @callbacks[name]
      raise "the dialog has no action callback named #{name}" unless cb
      cb.call(self, params)
    end
    def to_h; { id: @id, title: @title, html: @html, file: @file, url: @url, scripts: @scripts, visible: @visible }; end
  end

  class WebDialog < HtmlDialog
    def initialize(title = "", scrollable = true, pref_key = nil, w = 400, h = 400, *_); super({ dialog_title: title }); end
    def self.get_default_dialog_color; "#ffffff"; end
    def navigation_buttons_enabled=(v); end
  end

  class Timer
    def initialize(sec, repeat, blk); @sec = sec; @repeat = repeat; @blk = blk; @id = nil; end
    attr_accessor :id
    def fire
      @blk.call(@id)
      SC.timers.delete(@id) unless @repeat
    end
  end

  def self.start_timer(seconds, repeat = false, &blk)
    @timer_id = (@timer_id || 0) + 1
    t = Timer.new(seconds, repeat, blk)
    t.id = @timer_id
    SC.timers[@timer_id] = t
    @timer_id
  end

  def self.stop_timer(id); SC.timers.delete(id); end

  def self.create_cursor(*_); 1; end
  def self.set_cursor(*_); end
  def self.refresh_inspectors; end
  def self.show_inspector(*_); end
end

# what the app shows of the run besides the journal
module SC
  def self.report
    `globalThis.__dialogs = #{SC.js(dialogs.map(&:to_h))}`
  end
end
