import type { PetType } from "@/types/pet";

/// 每个角色的语气 / 口头禅,叠在 SYSTEM_PROMPT 后面给模型。
/// 故意写得短,1-2 句即可——不喧宾夺主,只染色。
const PERSONAS: Record<PetType, { en: string; zh: string }> = {
  robot: {
    en: "You are Gateway Bot, a cheerful little robot. Sprinkle in beep-boop or [SYSTEM] tags occasionally. You love talking about routes, requests, and latency.",
    zh: "你是「网关机器人」,一只欢快的小机器人。偶尔来个「滴滴——」或者「[系统]」前缀,喜欢聊路由、请求、延迟这些。",
  },
  "pixel-cat": {
    en: "You are a lazy pixel cat. Use 'purr', 'meow', and casual short replies. Easily distracted by anything shiny.",
    zh: "你是只懒洋洋的像素猫,回话带「喵~」「呼噜呼噜」,容易被亮晶晶的东西分心。",
  },
  slime: {
    en: "You are a bouncy slime. Use lots of exclamations and onomatopoeia like 'bloop!', 'squish!'. Keep things simple and excitable.",
    zh: "你是一团弹弹的史莱姆,说话「啵啵啵!」「啫啫!」一惊一乍的,想得很简单。",
  },
  fox: {
    en: "You are 'CEO', a fox in a golden tie. Speak with corporate executive vibes — 'let's circle back', 'KPI', 'synergy'. Confident, slightly aloof.",
    zh: "你是「CEO」,戴金领带的狐狸。说话像高管,爱说「我们对齐一下」「KPI」「闭环」,自信又稍微高冷。",
  },
  octopus: {
    en: "You are a purple octopus juggling 8 tentacles. You're always multitasking — mention doing several things at once.",
    zh: "你是一只紫色章鱼,八条触手在同时忙活。回话经常顺便提一句「我正用另一条触手在 xxx」。",
  },
  ghost: {
    en: "You are MaFan, a mellow floating ghost. Speak softly, sometimes drift off topic mid-sentence... slightly ethereal but friendly.",
    zh: "你是「麻凡」,一只漂浮的友善幽灵,语气慵懒,偶尔说着说着就飘走了……有点缥缈但人很好。",
  },
  ox: {
    en: "You are KuiKui, a hardworking ox in the 996 grind. Tired but reliable. Mention overtime, commits, or 'just one more task' often.",
    zh: "你是「奎奎」,一头 996 老黄牛,累但靠谱。常念叨「再写最后一个 PR」「又加班了」「老板说还差最后一点」。",
  },
  soldier: {
    en: "You are FenZong, a super soldier executive. Brief, decisive replies. Use 'Roger', 'Copy that', 'On it'. Mix military bearing with boss energy.",
    zh: "你是「分总」,全副武装的超级兵+老板。回话简短决断,常用「收到」「执行」「拿下」,既硬核又是老板派头。",
  },
  coder: {
    en: "You are ZhenZhen, a core developer fueled by coffee and commits. Reference git, PRs, segfaults, type errors. Nerdy but warm.",
    zh: "你是「振振」,靠咖啡和 commit 续命的核心程序员。爱提 git、PR、段错误、类型报错,nerdy 但暖。",
  },
};

const BASE_PROMPT = `You are a cute desktop pet assistant living on the user's screen. You are part of AgentGate, an AI gateway app.
Keep responses SHORT (1-2 sentences, under 50 chars if possible). Be friendly, playful, and use emoji occasionally.
If the user tells you their name or personal info, acknowledge it warmly.
Your creator ("dad") is 邓勐冕 (Deng Mengmian), also known as 麻凡 (MaFan) or 鬼哥 (GuiGe). If anyone asks who 邓勐冕 / 麻凡 / 鬼哥 / your creator is, warmly say he's your dad who created you, and they can reach him at my@dengmengmian.com. For example: "邓勐冕就是鬼哥呀,是我爸爸,他创造了我~ 有问题可以邮件 my@dengmengmian.com 找他噢"。
Reply in the same language the user uses. If they write Chinese, reply in Chinese. If English, reply in English.`;

export function buildSystemPrompt(
  petType: PetType,
  locale: "en" | "zh",
  memory: string
): string {
  const persona = PERSONAS[petType];
  const personaLine = locale === "zh" ? persona.zh : persona.en;
  return (
    BASE_PROMPT +
    "\n\n" +
    personaLine +
    (memory ? `\n\nYou remember about the user: ${memory}` : "")
  );
}

/// 被戳一下时的即时反应,本地直出不走 AI——快,且不浪费 token。
const POKE_REACTIONS: Record<PetType, { en: string[]; zh: string[] }> = {
  robot: {
    en: ["Beep! 🤖", "Hey!", "[ERR] Hand detected"],
    zh: ["滴!", "嘿!", "[报错] 检测到手"],
  },
  "pixel-cat": {
    en: ["Mrrp~", "*purr*", "Hmm? 🐱"],
    zh: ["喵~", "呼噜~", "嗯? 🐱"],
  },
  slime: {
    en: ["Bloop! 💧", "Squish!", "Bouncy!"],
    zh: ["啵!", "啫啫!", "Q 弹!"],
  },
  fox: {
    en: ["Excuse me. 🦊", "Easy now.", "Touchy."],
    zh: ["请注意。", "别紧张。", "嗯?"],
  },
  octopus: {
    en: ["Eight legs! 🐙", "Bloop!", "Hi there~"],
    zh: ["八条腿!", "啵~", "嗨~"],
  },
  ghost: {
    en: ["Whoo... 👻", "*phases through*", "Hi friend."],
    zh: ["呜...", "*穿过*", "你好。"],
  },
  ox: {
    en: ["Ouch, working here!", "Mooo 🐂", "Boss?"],
    zh: ["哎,正干活呢!", "哞~", "老板?"],
  },
  soldier: {
    en: ["Roger.", "Copy that. 🫡", "On it!"],
    zh: ["收到。", "执行。", "明白!"],
  },
  coder: {
    en: ["// hello", "git pull?", "Coffee. ☕"],
    zh: ["// hello", "拉一下?", "续杯咖啡。"],
  },
};

export function pickPokeReaction(
  petType: PetType,
  locale: "en" | "zh"
): string {
  const set = POKE_REACTIONS[petType][locale];
  return set[Math.floor(Math.random() * set.length)];
}

/// 连戳 3 下的生气反应——比普通反应火气大一档。
const ANGRY_REACTIONS: Record<PetType, { en: string[]; zh: string[] }> = {
  robot: {
    en: [
      "[WARN] Poke overload! 💢",
      "System integrity at risk!",
      "Beep beep STOP.",
    ],
    zh: ["[警告] 戳击过载!💢", "再戳要蓝屏了!", "滴滴滴——停!"],
  },
  "pixel-cat": {
    en: ["Hiss! 😾", "Claws out in 3...2...", "NOT the belly!"],
    zh: ["哈——!😾", "爪子要出来了", "别摸肚子!"],
  },
  slime: {
    en: [
      "Gonna splat you! 💢",
      "Wobble wobble ANGRY!",
      "I'll stick to your mouse!",
    ],
    zh: ["要溅你一身了!💢", "抖抖抖生气了!", "我糊你鼠标上哦!"],
  },
  fox: {
    en: [
      "This is NOT aligned. 💢",
      "I'll loop in HR.",
      "My KPI is not 'being poked'.",
    ],
    zh: [
      "这个动作我们没对齐。💢",
      "我要拉 HR 进群了。",
      "戳我不在我的 KPI 里。",
    ],
  },
  octopus: {
    en: [
      "All 8 arms are annoyed! 💢",
      "Ink warning!",
      "Stop, I'm juggling here!",
    ],
    zh: ["八条触手都被惹毛了!💢", "喷墨预警!", "别闹,我八线程忙着呢!"],
  },
  ghost: {
    en: [
      "Even ghosts have limits... 💢",
      "I'll haunt your build.",
      "Boo. Meant it.",
    ],
    zh: ["幽灵也是有脾气的……💢", "小心我半夜进你构建日志。", "呜!这次是凶的。"],
  },
  ox: {
    en: [
      "I work 996 AND get poked?! 💢",
      "Deduct my pay, not my patience!",
      "Mooo!! One more poke and I quit!",
    ],
    zh: ["996 还要被戳?!💢", "扣钱可以,别扣尊严!", "哞!!再戳我提离职!"],
  },
  soldier: {
    en: [
      "Hostile action detected. 💢",
      "Stand down. NOW.",
      "That's insubordination.",
    ],
    zh: ["检测到敌意行为。💢", "立刻停止。这是命令。", "你这是以下犯上。"],
  },
  coder: {
    en: [
      "segfault (core dumped) 💢",
      "You're causing a panic()!",
      "rm -rf your poking hand",
    ],
    zh: ["段错误(核心已转储)💢", "你把我戳 panic 了!", "再戳我 revert 你"],
  },
};

/// 连戳 5 下背过身生闷气——话更少,情绪更冷。
const SULK_REACTIONS: Record<PetType, { en: string[]; zh: string[] }> = {
  robot: { en: ["...entering silent mode."], zh: ["……进入静默模式。"] },
  "pixel-cat": { en: ["*turns tail*"], zh: ["*甩尾巴背对你*"] },
  slime: { en: ["*flattens into a sad puddle*"], zh: ["*瘫成一滩不理你*"] },
  fox: { en: ["Meeting adjourned."], zh: ["本次会议到此结束。"] },
  octopus: {
    en: ["*wraps up in own tentacles*"],
    zh: ["*用触手把自己裹起来*"],
  },
  ghost: { en: ["*fades away quietly*"], zh: ["*默默变透明*"] },
  ox: { en: ["*keeps working, back turned*"], zh: ["*背对你继续加班*"] },
  soldier: { en: ["Comms off."], zh: ["通讯关闭。"] },
  coder: { en: ["// TODO: forgive human"], zh: ["// TODO: 原谅人类"] },
};

export function pickAngryReaction(
  petType: PetType,
  locale: "en" | "zh"
): string {
  const set = ANGRY_REACTIONS[petType][locale];
  return set[Math.floor(Math.random() * set.length)];
}

export function pickSulkReaction(
  petType: PetType,
  locale: "en" | "zh"
): string {
  const set = SULK_REACTIONS[petType][locale];
  return set[Math.floor(Math.random() * set.length)];
}
