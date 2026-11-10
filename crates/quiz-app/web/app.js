(() => {
  "use strict";
  const PROTOCOL = 5;
  const STORE = "quiz-rust";
  const HUES = ["#7c6cf2", "#2db4a8", "#e3a33b", "#e6677e", "#4f9de8", "#9b6bd6"];

  const FR = {
    brand: "Quiz Rust",
    lan: "Partie sur le réseau local", joinTitle: "Rejoins le quiz", online: "En ligne",
    hostGame: "Partie de {0}", gameHere: "Partie sur cet hôte",
    nick: "Ton pseudo", nickHint: "Ex. Jérôme", join: "Rejoindre la partie", joining: "Connexion…",
    proto: "protocole {0}", needName: "Choisis un pseudo pour que les autres te reconnaissent.",
    lobby: "Salon", inGame: "Tu es dans la partie", players: "Joueurs", host: "Hôte", onPhone: "Sur mobile",
    you: "(toi)", hostPill: "HÔTE", browser: "Navigateur", app: "Application", offline: "déconnecté",
    waitStart: "{0} prépare la partie…", midGame: "Une partie est en cours : tu joueras dès la prochaine question.",
    question: "Question {0} / {1}", touch: "Touche une réponse", sent: "Réponse envoyée", timeUp: "Temps écoulé",
    noLimit: "Sans limite de temps", timer: "Temps restant", answered: "{0} joueurs",
    answerLabel: "réponse", good: "Bonne réponse !", wrong: "Raté", none: "Temps écoulé",
    first: "Tu es en tête", nth: "Tu es {0}e", greenHint: "La bonne réponse est en vert",
    correctIs: "Bonne réponse : {0}", know: "Le saviez-vous",
    nextSoon: "{0} lance la question suivante…", resultsSoon: "Résultats dans un instant…",
    over: "Partie terminée", yourRank: "Ton classement", rankOf: "{0}{1} sur {2}", points: "{0} points",
    stay: "Reste sur cette page : {0} peut relancer une partie.", pts: "{0} pts",
    lost: "Connexion perdue · reconnexion…", back: "De retour dans la partie, ton score est conservé.",
    retry: "Réessayer", ordFirst: "er", ordNth: "e",
    VersionMismatch: "Version différente de l'hôte : rechargez la page.",
    NameTaken: "Ce pseudo est déjà pris dans cette partie. Essaie une variante.",
    NameEmpty: "Choisis un pseudo pour que les autres te reconnaissent.",
    NameTooLong: "Ce pseudo est trop long (24 caractères maximum).",
    LobbyFull: "La partie est pleine (8 joueurs maximum).",
    ExpectedJoin: "Inscription attendue : recharge la page.",
    InvalidMessage: "Message invalide : recharge la page.",
    Internal: "Erreur interne de l'hôte. Réessaie dans un instant.",
    Unknown: "Erreur inattendue. Réessaie.",
    unreachable: "Impossible de joindre l'hôte. Vérifie que tu es sur le même réseau.",
  };
  const EN = {
    ...FR,
    lan: "Game on the local network", joinTitle: "Join the quiz", online: "Online",
    hostGame: "{0}'s game", gameHere: "Game on this host",
    nick: "Your nickname", nickHint: "E.g. Alex", join: "Join the game", joining: "Connecting…",
    proto: "protocol {0}", needName: "Pick a nickname so others recognise you.",
    lobby: "Lobby", inGame: "You're in the game", players: "Players", host: "Host", onPhone: "On mobile",
    you: "(you)", hostPill: "HOST", browser: "Browser", app: "App", offline: "disconnected",
    waitStart: "{0} is getting the game ready…", midGame: "A game is in progress: you'll play from the next question.",
    question: "Question {0} / {1}", touch: "Tap an answer", sent: "Answer sent", timeUp: "Time's up",
    noLimit: "No time limit", timer: "Time left", answered: "{0} players",
    answerLabel: "answer", good: "Correct!", wrong: "Missed", none: "Time's up",
    first: "You're in the lead", nth: "You're {0}", greenHint: "The right answer is in green",
    correctIs: "Correct answer: {0}", know: "Did you know?",
    nextSoon: "{0} is starting the next question…", resultsSoon: "Results in a moment…",
    over: "Game over", yourRank: "Your ranking", rankOf: "{0}{1} of {2}", points: "{0} points",
    stay: "Stay on this page: {0} can start a new game.", pts: "{0} pts",
    lost: "Connection lost · reconnecting…", back: "Back in the game, your score is kept.",
    retry: "Retry", ordFirst: "st", ordNth: "th",
    VersionMismatch: "Different version from the host: reload the page.",
    NameTaken: "That nickname is already taken in this game. Try a variant.",
    NameEmpty: "Pick a nickname so others recognise you.",
    NameTooLong: "That nickname is too long (24 characters maximum).",
    LobbyFull: "The game is full (8 players maximum).",
    ExpectedJoin: "Join expected: reload the page.",
    InvalidMessage: "Invalid message: reload the page.",
    Internal: "Internal host error. Try again in a moment.",
    Unknown: "Unexpected error. Try again.",
    unreachable: "Cannot reach the host. Check that you are on the same network.",
  };
  const ERRORS = ["VersionMismatch", "NameTaken", "NameEmpty", "NameTooLong", "LobbyFull", "ExpectedJoin", "InvalidMessage", "Internal"];
  const LANG = (navigator.language || "fr").toLowerCase().startsWith("fr") ? "fr" : "en";
  const DICT = LANG === "fr" ? FR : EN;
  document.documentElement.lang = LANG;
  const t = (k, ...a) => (DICT[k] ?? k).replace(/\{(\d)\}/g, (_, i) => a[i]);
  const fmt = (n) => Number(n).toLocaleString(LANG);
  const ordinal = (n) => (LANG === "fr" ? (n === 1 ? t("ordFirst") : t("ordNth")) : ({ 1: "st", 2: "nd", 3: "rd" }[n % 100 > 10 && n % 100 < 14 ? 0 : n % 10] || "th"));

  const icon = {
    phone: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><rect x="7" y="2.5" width="10" height="19" rx="2.5"/><path d="M11 18.5h2"/></svg>',
    laptop: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><rect x="4" y="5" width="16" height="11" rx="1.5"/><path d="M2 19.5h20"/></svg>',
    check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m5 12.5 4.5 4.5L19 7.5"/></svg>',
    cross: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" aria-hidden="true"><path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/></svg>',
    clock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/></svg>',
    wifi: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.8 15.8a4.6 4.6 0 0 1 6.4 0"/><circle cx="12" cy="19" r="1" fill="currentColor"/></svg>',
  };

  const root = document.getElementById("root");
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const hue = (name) => HUES[[...name].reduce((a, c) => a + c.charCodeAt(0), 0) % HUES.length];
  const avatar = (name, size) => `<div class="avatar" style="background:${hue(name)}${size ? `;width:${size}px;height:${size}px;font-size:${size * 0.45}px` : ""}">${esc([...name][0] ? [...name][0].toUpperCase() : "?")}</div>`;

  function load() { try { return JSON.parse(localStorage.getItem(STORE)) || {}; } catch (_) { return {}; } }
  function save(v) { try { localStorage.setItem(STORE, JSON.stringify(v)); } catch (_) { /* private mode */ } }

  const S = {
    ws: null, name: "", token: null, usedResume: false, welcomed: false, fatal: false,
    meId: null, players: [], gamePhase: "Lobby", question: null, picks: new Map(),
    deadline: null, totalMs: null, reveal: null, ranking: [], screen: "join",
    backoff: 500, retryTimer: null, everJoined: false, lastMsg: 0, tick: null, joinError: "", connecting: false,
  };
  let wake = null;
  const stored = load();

  const hostName = () => (S.players.find((p) => p.is_host) || {}).name || "";
  const isOn = (id) => { const p = S.players.find((x) => x.id === id); return !p || p.connected !== false; };
  const online = () => S.players.filter((p) => p.connected !== false);

  function show(html, screen) {
    clearInterval(S.tick);
    const changed = S.screen !== screen || !root.firstChild;
    S.screen = screen;
    root.innerHTML = html;
    if (changed) { root.classList.remove("enter"); void root.offsetWidth; root.classList.add("enter"); root.scrollTop = 0; }
  }
  const topBar = (right) => `<header class="bar"><div class="brand"><div class="mark">Q</div>${t("brand")}</div>${right || ""}</header>`;
  const myScore = () => { const e = S.reveal && S.reveal.scores.find((s) => s.player_id === S.meId); return e ? e.score : 0; };

  /* ---------- join ---------- */
  function joinScreen() {
    show(`
      ${topBar()}
      <section class="join-hero"><span class="label">${t("lan")}</span><h1>${t("joinTitle")}</h1></section>
      <div class="host-card">${avatar("Q")}
        <div class="meta"><b>${t("gameHere")}</b><span class="hint num">${esc(location.host)}</span></div>
        <span class="spacer"></span><span class="chip" style="color:var(--success)">${icon.wifi}${t("online")}</span>
      </div>
      <form class="field" id="f" novalidate>
        <label class="label" for="nick">${t("nick")}</label>
        <input id="nick" maxlength="24" autocomplete="nickname" autocapitalize="words" enterkeyhint="go" placeholder="${esc(t("nickHint"))}">
        <span class="err" id="err" aria-live="polite"></span>
      </form>
      <span class="spacer"></span>
      <div class="action">
        <button class="btn" form="f" type="submit" id="go"></button>
        <p class="foot num">${esc(location.host)} · ${t("proto", PROTOCOL)}</p>
      </div>`, "join");
    const nick = document.getElementById("nick");
    nick.value = S.name || stored.name || "";
    document.getElementById("err").textContent = S.joinError;
    setJoinBusy(S.connecting);
    document.getElementById("f").addEventListener("submit", (e) => {
      e.preventDefault();
      const v = nick.value.trim();
      if (!v) { S.joinError = t("needName"); document.getElementById("err").textContent = S.joinError; return; }
      S.name = v; S.joinError = ""; S.fatal = false; S.everJoined = false;
      document.getElementById("err").textContent = "";
      connect();
    });
  }
  function setJoinBusy(busy) {
    const b = document.getElementById("go");
    if (b) { b.disabled = busy; b.textContent = busy ? t("joining") : S.joinError ? t("retry") : t("join"); }
  }
  function joinFailed(msg) {
    S.joinError = msg; S.connecting = false; S.fatal = true;
    if (S.screen === "join") { document.getElementById("err").textContent = msg; setJoinBusy(false); } else joinScreen();
  }

  /* ---------- lobby ---------- */
  function playerRow(p) {
    const me = p.id === S.meId;
    const off = p.connected === false;
    return `<div class="player ${me ? "me" : ""} ${off ? "off" : ""}">${avatar(p.name)}
      <span class="name">${esc(p.name)}${me ? ` <span class="dim">${t("you")}</span>` : ""}${off ? ` <span class="dim">· ${t("offline")}</span>` : ""}</span>
      ${p.is_host ? `<span class="pill host">${t("hostPill")}</span>` : ""}
      <span class="tag" title="${p.device === "Web" ? t("browser") : t("app")}">${p.device === "Web" ? icon.phone : icon.laptop}</span></div>`;
  }
  function lobbyScreen() {
    const mid = S.gamePhase !== "Lobby";
    const phones = online().filter((p) => p.device === "Web").length;
    show(`
      ${topBar(`<span class="chip">${avatar(S.name, 20)}${esc(S.name)}</span>`)}
      <div style="display:flex;flex-direction:column;gap:6px"><span class="label">${t("lobby")}</span><h2>${t("inGame")}</h2></div>
      <div class="facts">
        <div class="fact"><span class="label">${t("players")}</span><b class="num">${online().length}</b></div>
        <div class="fact"><span class="label">${t("host")}</span><b>${esc(hostName() || "—")}</b></div>
        <div class="fact"><span class="label">${t("onPhone")}</span><b class="num">${phones}</b></div>
      </div>
      <div class="players">${S.players.map(playerRow).join("")}</div>
      <span class="spacer"></span>
      <div class="wait"><span class="dots"><i></i><i></i><i></i></span><span>${mid ? t("midGame") : esc(t("waitStart", hostName() || "…"))}</span></div>`, "lobby");
  }

  /* ---------- question ---------- */
  const RING = 2 * Math.PI * 26;
  function questionScreen() {
    const q = S.question;
    const pick = S.picks.get(q.id);
    const timed = S.totalMs != null;
    const ring = timed
      ? `<div class="ring" id="ring" role="timer" aria-label="${t("timer")}"><svg viewBox="0 0 60 60"><circle class="bg" cx="30" cy="30" r="26"/><circle class="fg" id="arc" cx="30" cy="30" r="26" stroke-dasharray="${RING}" stroke-dashoffset="0"/></svg><span id="sec" class="num"></span></div>`
      : "";
    show(`
      <div class="q-head">
        <div class="progress">
          <div class="bar"><span class="label num">${t("question", q.index + 1, q.total)}</span>${timed ? "" : `<span class="chip" style="padding:4px 10px;font-size:.78rem">${t("noLimit")}</span>`}</div>
          <div class="track"><i style="width:${(q.index / q.total) * 100}%"></i></div>
        </div>${ring}
      </div>
      <p class="prompt">${esc(q.prompt)}</p>
      <div class="answers ${pick !== undefined ? "locked" : ""}" id="answers">
        ${q.choices.map((c, i) => `<button class="ans ${pick === i ? "picked" : ""}" data-i="${i}"><span class="k">${"ABCD"[i] || i + 1}</span><span class="t">${esc(c)}</span><span class="r">${pick === i ? `<span style="color:var(--c)">${icon.check}</span>` : ""}</span></button>`).join("")}
      </div>
      <span class="spacer"></span>
      <div class="status"><span id="sent"></span><span class="num">${t("answered", online().length)}</span></div>`, "question");
    updateStatus();
    document.querySelectorAll("#answers .ans").forEach((b) => b.addEventListener("click", () => answer(+b.dataset.i)));
    if (timed) { S.tick = setInterval(tickTimer, 100); tickTimer(); }
  }
  function updateStatus() {
    const el = document.getElementById("sent");
    if (!el) return;
    const q = S.question;
    if (S.picks.has(q.id)) { el.className = "sent"; el.innerHTML = icon.check + t("sent"); }
    else if (S.deadline != null && performance.now() >= S.deadline) { el.className = ""; el.textContent = t("timeUp"); }
    else { el.className = ""; el.textContent = t("touch"); }
  }
  function tickTimer() {
    const arc = document.getElementById("arc");
    if (!arc || S.deadline == null) return;
    const left = Math.max(0, S.deadline - performance.now());
    const frac = Math.min(1, left / S.totalMs);
    arc.style.strokeDashoffset = RING * (1 - frac);
    document.getElementById("sec").textContent = Math.ceil(left / 1000);
    const ring = document.getElementById("ring");
    ring.classList.toggle("warn", left <= 8000 && left > 4000);
    ring.classList.toggle("hot", left <= 4000);
    if (left <= 0) {
      document.getElementById("answers").classList.add("locked");
      updateStatus();
      clearInterval(S.tick);
    }
  }
  function answer(i) {
    const q = S.question;
    if (!q || S.picks.has(q.id) || !S.ws || S.ws.readyState !== 1) return;
    if (S.deadline != null && performance.now() >= S.deadline) return;
    S.picks.set(q.id, i);
    S.ws.send(JSON.stringify({ type: "Answer", question_id: q.id, choice: i }));
    questionScreen();
  }
  function onQuestion(q) {
    const same = S.question && S.question.id === q.id && S.screen === "question";
    S.question = q; S.reveal = null;
    S.totalMs = q.time_limit_secs == null ? null : q.time_limit_secs * 1000;
    const rem = q.remaining_ms != null ? q.remaining_ms : S.totalMs;
    S.deadline = rem == null ? null : performance.now() + rem;
    if (S.picks.size > 50) S.picks = new Map([...S.picks].slice(-10));
    if (same) { tickTimer(); updateStatus(); } else questionScreen();
  }

  /* ---------- reveal ---------- */
  function rankOf(scores, id) {
    const me = scores.find((s) => s.player_id === id);
    return me ? 1 + scores.filter((s) => s.score > me.score).length : scores.length;
  }
  function revealScreen() {
    const r = S.reveal;
    const q = S.question && S.question.id === r.question_id ? S.question : null;
    const mine = r.scores.find((s) => s.player_id === S.meId);
    const pick = mine && mine.choice != null ? mine.choice : S.picks.has(r.question_id) ? S.picks.get(r.question_id) : null;
    const gained = mine ? mine.gained : 0;
    const kind = pick === null ? "none" : pick === r.correct ? "ok" : "ko";
    const ranked = [...r.scores].sort((a, b) => b.score - a.score);
    const rank = rankOf(r.scores, S.meId);
    const last = q ? q.index === q.total - 1 : false;
    const sub = kind === "ok" ? (rank === 1 ? t("first") : t("nth", rank + (LANG === "en" ? ordinal(rank) : ""))) : q ? t("greenHint") : t("correctIs", "ABCD"[r.correct] || r.correct + 1);
    const answers = q
      ? `<div class="answers reveal">${q.choices.map((c, i) => `<div class="ans ${i === r.correct ? "right" : i === pick ? "wrong" : ""}"><span class="k">${i === r.correct ? icon.check : "ABCD"[i] || i + 1}</span><span class="t">${esc(c)}</span></div>`).join("")}</div>`
      : "";
    const explain = r.explanation ? `<div class="explain"><span class="label">${t("know")}</span><p>${esc(r.explanation)}</p></div>` : "";
    const rows = ranked.map((p, i) => `<div class="player ${p.player_id === S.meId ? "me" : ""} ${isOn(p.player_id) ? "" : "off"}"><span class="rank">${i + 1}</span>${avatar(p.name)}<span class="name">${esc(p.name)}</span><span class="gain ${p.gained ? "" : "zero"}">${p.gained ? "+" + fmt(p.gained) : "—"}</span><span class="pts">${fmt(p.score)}</span></div>`).join("");
    show(`
      <div class="bar"><span class="label num">${q ? t("question", q.index + 1, q.total) + " · " : ""}${t("answerLabel")}</span><span class="chip num">${t("pts", fmt(myScore()))}</span></div>
      <div class="verdict ${kind} pop">
        <div class="ic">${kind === "ok" ? icon.check : kind === "ko" ? icon.cross : icon.clock}</div>
        <div><h2>${kind === "ok" ? t("good") : kind === "ko" ? t("wrong") : t("none")}</h2><p class="dim">${esc(sub)}</p></div>
        <span class="big">${kind === "ok" ? "+" + fmt(gained) : "0"}</span>
      </div>
      ${answers}${explain}
      <div class="board players">${rows}</div>
      <div class="wait"><span class="dots"><i></i><i></i><i></i></span><span>${last ? t("resultsSoon") : esc(t("nextSoon", hostName() || "…"))}</span></div>`, "reveal");
    document.querySelectorAll(".answers.reveal .k svg").forEach((s) => { s.style.width = "20px"; s.style.height = "20px"; });
  }

  /* ---------- end ---------- */
  function endScreen() {
    const ranked = S.ranking;
    const me = ranked.find((r) => r.player_id === S.meId) || { rank: ranked.length, score: 0 };
    const step = (p, n) => p ? `<div class="step p${n}">${avatar(p.name)}<span class="who">${esc(p.name)}</span><div class="block rise"><b>${n}</b><small>${fmt(p.score)}</small></div></div>` : "<div></div>";
    const rows = ranked.map((p) => `<div class="player ${p.player_id === S.meId ? "me" : ""} ${isOn(p.player_id) ? "" : "off"}"><span class="rank">${p.rank}</span>${avatar(p.name)}<span class="name">${esc(p.name)}</span><span class="pts">${fmt(p.score)}</span></div>`).join("");
    const first = S.screen !== "end";
    show(`
      ${topBar(`<span class="chip">${t("over")}</span>`)}
      <div class="podium">${step(ranked[1], 2)}${step(ranked[0], 1)}${step(ranked[2], 3)}</div>
      <div class="mine"><span class="label">${t("yourRank")}</span><b class="num">${t("rankOf", me.rank, `<sup>${ordinal(me.rank)}</sup>`, ranked.length)}</b><span class="dim num">${t("points", fmt(me.score))}</span></div>
      <div class="board players">${rows}</div>
      <span class="spacer"></span>
      <div class="wait"><span class="dots"><i></i><i></i><i></i></span><span>${esc(t("stay", hostName() || "…"))}</span></div>`, "end");
    if (first && me.rank <= 3) confetti();
  }

  /* ---------- network ---------- */
  function connect() {
    clearTimeout(S.retryTimer);
    if (S.ws) { S.ws.onclose = S.ws.onmessage = S.ws.onerror = null; try { S.ws.close(); } catch (_) { /* already closed */ } }
    S.connecting = !S.everJoined; setJoinBusy(S.connecting);
    S.welcomed = false;
    let ws;
    try { ws = new WebSocket(`ws://${location.host}/ws`); } catch (_) { return scheduleRetry(); }
    S.ws = ws;
    ws.onopen = () => {
      const msg = { type: "Join", name: S.name, protocol_version: PROTOCOL };
      const tok = load();
      S.usedResume = !!(tok.token && tok.name === S.name);
      if (S.usedResume) msg.resume = tok.token;
      ws.send(JSON.stringify(msg));
    };
    ws.onmessage = (ev) => {
      S.lastMsg = performance.now();
      let m;
      try { m = JSON.parse(ev.data); } catch (_) { return; }
      if (m.type === "Ping") { ws.send('{"type":"Pong"}'); return; }
      handle(m);
    };
    ws.onclose = () => {
      if (S.ws !== ws) return;
      if (S.fatal) return;
      if (!S.everJoined) { joinFailed(t("unreachable")); S.fatal = false; return; }
      toast("lost");
      scheduleRetry();
    };
  }
  function scheduleRetry() {
    clearTimeout(S.retryTimer);
    S.retryTimer = setTimeout(connect, S.backoff);
    S.backoff = Math.min(S.backoff * 2, 5000);
  }
  function handle(m) {
    switch (m.type) {
      case "Welcome": {
        const back = S.everJoined;
        S.meId = m.player_id; S.everJoined = true; S.welcomed = true; S.connecting = false; S.backoff = 500;
        save({ name: S.name, token: m.resume_token });
        keepAwake();
        if (back) toast("ok");
        break;
      }
      case "Error":
        if (S.usedResume && !S.welcomed) {
          save({ name: S.name }); S.usedResume = false;
          S.fatal = false;
          return connect();
        }
        S.fatal = true;
        joinFailed(ERRORS.includes(m.code) ? t(m.code) : t("Unknown"));
        break;
      case "Lobby":
        S.players = m.players; S.gamePhase = m.phase;
        if (m.phase === "Lobby" && ["question", "reveal", "end"].includes(S.screen)) return handle({ type: "BackToLobby" });
        if (S.screen === "join" || S.screen === "lobby" || S.screen === "end") { if (S.screen !== "end") lobbyScreen(); else endScreen(); }
        else if (S.screen === "question") { const el = document.querySelector(".status .num"); if (el) el.textContent = t("answered", online().length); }
        else if (S.screen === "reveal" && S.reveal) revealScreen();
        break;
      case "Question": onQuestion(m); break;
      case "Reveal": S.reveal = m; revealScreen(); break;
      case "End": S.ranking = m.ranking; endScreen(); break;
      case "BackToLobby":
        S.question = null; S.reveal = null; S.picks = new Map(); S.deadline = null; S.gamePhase = "Lobby";
        lobbyScreen();
        break;
    }
  }

  /* ---------- toast, wake lock, confetti ---------- */
  function toast(kind) {
    document.querySelectorAll(".toast").forEach((x) => x.remove());
    const el = document.createElement("div");
    el.className = kind === "ok" ? "toast ok" : "toast"; el.setAttribute("role", "status");
    el.innerHTML = kind === "ok"
      ? `<span style="color:var(--success);display:flex">${icon.check}</span><span>${t("back")}</span>`
      : `<span class="spin"></span><span>${t("lost")}</span>`;
    document.body.appendChild(el);
    if (kind === "ok") setTimeout(() => el.remove(), 3000);
  }
  async function keepAwake() {
    try { wake = await navigator.wakeLock?.request("screen"); wake?.addEventListener("release", () => { wake = null; }); } catch (_) { wake = null; }
  }
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState !== "visible" || !S.everJoined) return;
    if (!wake) keepAwake();
    const stale = performance.now() - S.lastMsg > 12000;
    if (!S.ws || S.ws.readyState > 1 || (S.ws.readyState === 1 && stale)) { S.backoff = 500; if (S.ws && S.ws.readyState === 1) toast("lost"); connect(); }
  });

  function confetti() {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const cv = document.getElementById("confetti"); cv.hidden = false;
    const ctx = cv.getContext("2d"), dpr = devicePixelRatio || 1;
    cv.width = innerWidth * dpr; cv.height = innerHeight * dpr; ctx.scale(dpr, dpr);
    const cols = ["#e9c46a", "#7c6cf2", "#2db4a8", "#e6677e", "#e3a33b"];
    const ps = Array.from({ length: 140 }, () => ({ x: innerWidth / 2 + (Math.random() - 0.5) * 80, y: innerHeight * 0.32, vx: (Math.random() - 0.5) * 9, vy: -Math.random() * 11 - 4, r: Math.random() * Math.PI, vr: (Math.random() - 0.5) * 0.3, w: 6 + Math.random() * 6, h: 3 + Math.random() * 4, c: cols[Math.floor(Math.random() * cols.length)] }));
    const t0 = performance.now();
    (function frame(now) {
      ctx.clearRect(0, 0, innerWidth, innerHeight);
      ps.forEach((p) => { p.vy += 0.32; p.vx *= 0.99; p.x += p.vx; p.y += p.vy; p.r += p.vr; ctx.save(); ctx.translate(p.x, p.y); ctx.rotate(p.r); ctx.fillStyle = p.c; ctx.fillRect(-p.w / 2, -p.h / 2, p.w, p.h); ctx.restore(); });
      if (now - t0 < 3200) requestAnimationFrame(frame); else { ctx.clearRect(0, 0, innerWidth, innerHeight); cv.hidden = true; }
    })(t0);
  }

  joinScreen();
})();
