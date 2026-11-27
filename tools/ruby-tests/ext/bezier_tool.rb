# Written like Bezier Spline: a tool where each click adds a control point, the VCB takes the number of
# segments, and Return draws the curve. Uses Sketchup::Tool, view drawing, the VCB and add_curve.
module SCFixtures
  class BezierTool
    def initialize; @pts = []; @segs = 12; end

    def activate
      Sketchup.status_text = 'Click control points, Return to finish'
      Sketchup.vcb_label = 'Segments'
      Sketchup.vcb_value = @segs
    end

    def enableVCB?; true; end

    def onLButtonDown(flags, x, y, view)
      ip = view.inputpoint(x, y)
      @pts << ip.position if ip.valid?
      view.invalidate
    end

    def onUserText(text, view)
      @segs = [text.to_i, 2].max
      Sketchup.vcb_value = @segs
    end

    def onReturn(view)
      return if @pts.length < 3
      model = view.model
      model.start_operation('Bezier', true)
      model.active_entities.add_curve(curve(@pts, @segs))
      model.commit_operation
      @pts = []
    end

    def onCancel(reason, view); @pts = []; end

    def draw(view)
      view.line_stipple = '-'
      view.draw_polyline(@pts) if @pts.length > 1
    end

    # de Casteljau
    def curve(ctrl, n)
      (0..n).map do |i|
        t = i.to_f / n
        pts = ctrl.map(&:to_a)
        while pts.length > 1
          pts = pts.each_cons(2).map { |a, b| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t] }
        end
        Geom::Point3d.new(pts[0])
      end
    end
  end
end

UI.menu('Plugins').add_item('Bezier tool') do
  Sketchup.active_model.select_tool(SCFixtures::BezierTool.new)
end
