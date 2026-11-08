(() => {
  'use strict';

  const $ = id => document.getElementById(id);
  const canvas = $('chart');
  const ctx = canvas.getContext('2d');
  const tooltip = $('tooltip');
  const records = [];
  const recordIds = new Set();

  let samples = [];
  let events = [];
  let cursor = 0;
  let windowSeconds = 60;
  let state = null;
  let serverOnline = false;
  let hasResponse = false;
  let responseAt = 0;
  let paused = false;
  let frozenEnd = 0;
  let hoverX = null;
  let layout = null;
  let drawQueued = false;
  let visibleSamples = [];
  let markerCount = 0;

  const timeFormat = new Intl.DateTimeFormat('zh-CN', {
    timeZone: 'Asia/Shanghai',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
  const shortTime = value => timeFormat.format(new Date(value * 1000));
  const currentTime = () => state
    ? state.now + (performance.now() - responseAt) / 1000
    : Date.now() / 1000;
  const normalizeState = value => String(value || '').toUpperCase();
  const isConnected = value => ['CONNECTED', 'OPEN', 'READY'].includes(normalizeState(value));

  function queueDraw() {
    if (!drawQueued) {
      drawQueued = true;
      requestAnimationFrame(() => {
        drawQueued = false;
        draw();
      });
    }
  }

  function ingest(incoming) {
    let changed = false;
    for (const record of incoming || []) {
      if (!Number.isFinite(record.id) || !Number.isFinite(record.ts) || recordIds.has(record.id)) {
        continue;
      }
      if (
        record.type === 'sample'
        && (!Number.isFinite(record.rssi) || record.rssi < -127 || record.rssi > 20)
      ) {
        continue;
      }
      recordIds.add(record.id);
      records.push(record);
      changed = true;
    }
    if (!changed) return;

    records.sort((a, b) => a.id - b.id);
    while (records.length > 100000) {
      recordIds.delete(records.shift().id);
    }
    rebuild();
  }

  function rebuild() {
    samples = [];
    events = [];
    let segment = 0;
    let previous = null;
    let ema = null;

    for (const record of records) {
      if (record.type === 'sample') {
        if (
          previous
          && (
            record.ts - previous.ts > 3.5
            || record.ts < previous.ts
            || record.peer !== previous.peer
          )
        ) {
          segment++;
          ema = null;
        }
        ema = ema === null ? record.rssi : record.rssi * 0.2 + ema * 0.8;
        samples.push({ ...record, ema, segment });
        previous = record;
      } else if (record.type === 'link' || record.type === 'serial') {
        events.push(record);
        if (!isConnected(record.state)) {
          segment++;
          ema = null;
          previous = null;
        }
      } else if (record.type === 'marker') {
        events.push(record);
      }
    }
  }

  function updateStatus() {
    const now = currentTime();
    const age = state && state.last_sample_ts !== null
      ? Math.max(0, now - state.last_sample_ts)
      : Infinity;
    const fresh = serverOnline
      && state
      && state.serial_connected
      && state.ble_state === 'CONNECTED'
      && age <= 3.5
      && Number.isFinite(state.last_rssi);

    $('reading').textContent = fresh ? state.last_rssi : '等待新读数';
    $('reading').classList.toggle('waiting', !fresh);
    $('unit').hidden = !fresh;

    if (!hasResponse) {
      $('freshness').textContent = '正在连接采样服务…';
    } else if (fresh) {
      $('freshness').textContent = age < 1.5 ? '刚刚更新' : `${Math.floor(age)} 秒前更新`;
    } else if (state && state.last_sample_ts !== null && Number.isFinite(state.last_rssi)) {
      $('freshness').textContent = `上一读数 ${state.last_rssi} dBm · ${shortTime(state.last_sample_ts)}`;
    } else {
      $('freshness').textContent = '尚未收到信号样本';
    }

    let text = '连接中';
    if (hasResponse && !serverOnline) {
      text = '采样服务连接中断';
    } else if (state && !state.serial_connected) {
      text = '开发板未连接';
    } else if (state && state.ble_state === 'CONNECTED') {
      text = fresh ? '手机已连接' : '手机已连接 · 等待读数';
    } else if (state) {
      text = '等待手机连接';
    }
    $('connection-text').textContent = text;
    $('connection').classList.toggle('warn', !fresh);
    $('peer').textContent = state && state.peer ? state.peer : '';
    $('marker-button').disabled = !serverOnline;
    $('live-note').textContent = paused ? '画面已暂停，仍在记录' : '实时记录';
    $('live-note').classList.toggle('paused', paused);
    $('error-note').textContent = !serverOnline && hasResponse
      ? '正在自动重连采样服务，已显示的历史会保留。'
      : state && state.error ? state.error : '';
  }

  function resize() {
    const rect = canvas.getBoundingClientRect();
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(1, Math.round(rect.width * ratio));
    const h = Math.max(1, Math.round(rect.height * ratio));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    return { width: rect.width, height: rect.height };
  }

  function eventLabel(event) {
    if (event.type === 'marker') return event.label || '位置标记';
    if (event.type === 'serial') return isConnected(event.state) ? '开发板连接' : '开发板断开';
    if (event.state === 'CONNECTED') return '手机连接';
    if (event.state === 'ADVERTISING') return '等待手机';
    return '手机断开';
  }

  function drawLine(points, field, x, y, color, width) {
    if (!points.length) return;

    ctx.strokeStyle = color;
    ctx.lineWidth = width;
    ctx.lineJoin = 'round';
    ctx.lineCap = 'round';
    ctx.beginPath();
    let previous = null;
    for (const sample of points) {
      const px = x(sample.ts);
      const py = y(sample[field]);
      if (!previous || sample.segment !== previous.segment) {
        ctx.moveTo(px, py);
      } else {
        ctx.lineTo(px, py);
      }
      previous = sample;
    }
    ctx.stroke();

    if (points.length === 1) {
      ctx.beginPath();
      ctx.arc(x(points[0].ts), y(points[0][field]), 2.5, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();
    }
  }

  function compact(points, field, x) {
    if (points.length < 2500) return points;

    const buckets = new Map();
    for (const point of points) {
      const key = `${point.segment}:${Math.floor(x(point.ts))}`;
      const bucket = buckets.get(key);
      if (!bucket) {
        buckets.set(key, { first: point, min: point, max: point, last: point });
      } else {
        bucket.last = point;
        if (point[field] < bucket.min[field]) bucket.min = point;
        if (point[field] > bucket.max[field]) bucket.max = point;
      }
    }

    const output = [];
    for (const bucket of buckets.values()) {
      const unique = new Map(
        [bucket.first, bucket.min, bucket.max, bucket.last].map(p => [p.id, p]),
      );
      output.push(...Array.from(unique.values()).sort((a, b) => a.ts - b.ts));
    }
    return output;
  }

  function draw() {
    updateStatus();
    const { width, height } = resize();
    ctx.clearRect(0, 0, width, height);

    const margin = { left: 54, right: 21, top: 37, bottom: 40 };
    const plotW = width - margin.left - margin.right;
    const plotH = height - margin.top - margin.bottom;
    if (plotW < 10 || plotH < 10) return;

    const end = paused ? frozenEnd : currentTime();
    const first = samples.length ? samples[0].ts : end - 60;
    const start = windowSeconds === 'all' ? Math.min(first, end - 10) : end - windowSeconds;
    visibleSamples = samples.filter(s => s.ts >= start && s.ts <= end);

    const minValue = visibleSamples.reduce((v, s) => Math.min(v, s.rssi), -100);
    const maxValue = visibleSamples.reduce((v, s) => Math.max(v, s.rssi), 0);
    const yMin = Math.floor(minValue / 10) * 10;
    const yMax = Math.ceil(maxValue / 10) * 10;
    const x = t => margin.left + (t - start) / (end - start) * plotW;
    const y = v => margin.top + (yMax - v) / (yMax - yMin) * plotH;
    layout = { width, height, margin, plotW, plotH, start, end, x, y };

    ctx.font = '11px -apple-system,BlinkMacSystemFont,"PingFang SC",sans-serif';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';
    const step = yMax - yMin > 110 ? 25 : 20;
    for (let value = yMax; value >= yMin; value -= step) {
      const py = y(value);
      ctx.strokeStyle = '#e7ece9';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(margin.left, py);
      ctx.lineTo(width - margin.right, py);
      ctx.stroke();
      ctx.fillStyle = '#8b999d';
      ctx.fillText(value, margin.left - 12, py);
    }

    ctx.textAlign = 'left';
    ctx.fillStyle = '#8b999d';
    ctx.fillText('dBm', margin.left - 39, 15);
    ctx.textAlign = 'right';
    ctx.fillText('较强', width - margin.right, 15);
    const tickCount = width < 500 ? 3 : 5;
    ctx.textAlign = 'center';
    for (let i = 0; i <= tickCount; i++) {
      const t = start + (end - start) * i / tickCount;
      ctx.fillText(shortTime(t), x(t), height - 16);
    }

    ctx.save();
    ctx.beginPath();
    ctx.rect(margin.left, margin.top, plotW, plotH);
    ctx.clip();
    const eventVisible = events.filter(e => e.ts >= start && e.ts <= end);
    for (const event of eventVisible) {
      const px = x(event.ts);
      const marker = event.type === 'marker';
      ctx.strokeStyle = marker ? '#c49b58' : '#c9877e';
      ctx.setLineDash(marker ? [4, 4] : [2, 5]);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(px, margin.top);
      ctx.lineTo(px, height - margin.bottom);
      ctx.stroke();
      ctx.setLineDash([]);
    }
    drawLine(compact(visibleSamples, 'rssi', x), 'rssi', x, y, '#9faeb3', 1.2);
    if ($('smooth').checked) {
      drawLine(compact(visibleSamples, 'ema', x), 'ema', x, y, '#187e87', 2.3);
    }

    let labelRight = -Infinity;
    ctx.font = '11px -apple-system,BlinkMacSystemFont,"PingFang SC",sans-serif';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    for (const event of eventVisible) {
      const px = x(event.ts);
      const label = eventLabel(event);
      if (px < labelRight + 12) continue;
      const text = label.length > 14 ? label.slice(0, 14) + '…' : label;
      const tw = ctx.measureText(text).width;
      ctx.fillStyle = event.type === 'marker' ? '#a47c3c' : '#ae7771';
      ctx.fillText(text, px + 5, margin.top + 7);
      labelRight = px + tw + 5;
    }
    ctx.restore();

    $('empty').hidden = visibleSamples.length > 0;
    if (visibleSamples.length === 0) {
      $('empty').style.display = 'flex';
      $('empty').querySelector('strong').textContent = samples.length
        ? '这个时段没有读数'
        : '等待手机信号';
      $('empty').querySelector('span').textContent = samples.length
        ? '断连期间会保留曲线空白'
        : '保持手机与 Kivo RSSI Demo 连接';
    } else {
      $('empty').style.display = 'none';
    }
    renderHover();
  }

  function renderHover() {
    if (hoverX === null || !layout || !visibleSamples.length) {
      tooltip.style.display = 'none';
      return;
    }

    const { x, y, start, end, margin, plotW, height, width } = layout;
    const time = start + (hoverX - margin.left) / plotW * (end - start);
    let lo = 0;
    let hi = visibleSamples.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (visibleSamples[mid].ts < time) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }

    const choices = [visibleSamples[lo], visibleSamples[lo - 1]].filter(Boolean);
    const point = choices.reduce(
      (best, p) => !best || Math.abs(p.ts - time) < Math.abs(best.ts - time) ? p : best,
      null,
    );
    if (
      !point
      || Math.abs(point.ts - time) > Math.max(3.5, (end - start) / plotW * 10)
      || hoverX < margin.left
      || hoverX > width - margin.right
    ) {
      tooltip.style.display = 'none';
      return;
    }

    const px = x(point.ts);
    const py = y(point.rssi);
    ctx.save();
    ctx.strokeStyle = '#c8d4d5';
    ctx.setLineDash([3, 4]);
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(px, margin.top);
    ctx.lineTo(px, height - margin.bottom);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = '#187e87';
    ctx.strokeStyle = '#fff';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(px, py, 4, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();

    $('tooltip-value').textContent = `${point.rssi} dBm`;
    $('tooltip-time').textContent = shortTime(point.ts);
    tooltip.style.display = 'block';
    const plotRect = $('plot').getBoundingClientRect();
    const canvasRect = canvas.getBoundingClientRect();
    tooltip.style.left = `${Math.min(
      width - tooltip.offsetWidth - 5,
      Math.max(4, px + canvasRect.left - plotRect.left + 12),
    )}px`;
    tooltip.style.top = `${Math.max(8, Math.min(height - tooltip.offsetHeight - 8, py - 55))}px`;
  }

  async function poll() {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);
    try {
      const response = await fetch(`/api/state?after=${cursor}`, {
        cache: 'no-store',
        signal: controller.signal,
      });
      if (!response.ok) throw new Error('服务暂不可用');
      const data = await response.json();
      if (!data.state || !Number.isFinite(data.cursor) || !Array.isArray(data.records)) {
        throw new Error('采样数据格式异常');
      }
      if (data.cursor < cursor) {
        records.length = 0;
        recordIds.clear();
        samples = [];
        events = [];
        cursor = 0;
        return;
      }
      ingest(data.records);
      cursor = data.cursor;
      state = data.state;
      responseAt = performance.now();
      serverOnline = true;
      hasResponse = true;
    } catch (error) {
      serverOnline = false;
      hasResponse = true;
    } finally {
      clearTimeout(timeout);
      queueDraw();
      setTimeout(poll, 1000);
    }
  }

  document.querySelectorAll('[data-window]').forEach(button => {
    button.addEventListener('click', () => {
      windowSeconds = button.dataset.window === 'all' ? 'all' : Number(button.dataset.window);
      document.querySelectorAll('[data-window]').forEach(b => {
        b.setAttribute('aria-pressed', String(b === button));
      });
      hoverX = null;
      queueDraw();
    });
  });

  $('smooth').addEventListener('change', queueDraw);

  $('pause').addEventListener('click', () => {
    paused = !paused;
    if (paused) frozenEnd = currentTime();
    $('pause').textContent = paused ? '继续实时' : '暂停画面';
    hoverX = null;
    queueDraw();
  });

  $('marker-form').addEventListener('submit', async event => {
    event.preventDefault();
    const label = $('marker-label').value.trim() || `位置 ${++markerCount}`;
    $('marker-button').disabled = true;
    try {
      const response = await fetch('/api/marker', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ label }),
      });
      if (!response.ok) throw new Error();
      const record = await response.json();
      ingest([record]);
      $('marker-label').value = '';
      $('marker-result').textContent = `已标记「${label}」 · ${shortTime(record.ts)}`;
      queueDraw();
    } catch (error) {
      $('marker-result').textContent = '标记未保存，请重试。';
    } finally {
      $('marker-button').disabled = !serverOnline;
    }
  });

  canvas.addEventListener('pointermove', event => {
    hoverX = event.clientX - canvas.getBoundingClientRect().left;
    queueDraw();
  });
  canvas.addEventListener('pointerleave', () => {
    hoverX = null;
    queueDraw();
  });

  new ResizeObserver(queueDraw).observe($('plot'));
  setInterval(queueDraw, 500);
  poll();
})();
