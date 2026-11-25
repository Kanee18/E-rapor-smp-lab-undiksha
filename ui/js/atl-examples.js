// Contoh indikator ATL & Learner Profile per predikat (bahasa Inggris, bisa diubah di Pengaturan).
// EE = paling tinggi (konsisten, mandiri, melampaui), BE = masih perlu banyak bimbingan.
// Kunci = id atribut bawaan; atribut buatan sendiri dicocokkan lewat namanya.
export const ATL_LEVEL_EXAMPLES = {
  inquirer: {
    EE: 'Shows exceptional curiosity; consistently asks insightful, open-ended questions and independently drives inquiry beyond the task.',
    ME: 'Demonstrates natural curiosity, asks open-ended questions, and actively engages in inquiry cycles.',
    AE: 'Shows curiosity at times; asks questions and engages in inquiry with some guidance from the teacher.',
    BE: 'Rarely asks questions or engages in inquiry; needs regular prompting and support to explore ideas.',
  },
  caring: {
    EE: 'Highly empathetic; consistently helps peers who struggle and shows genuine kindness and respect in every setting.',
    ME: 'Shows empathy and kindness; regularly helps peers and works respectfully during group work.',
    AE: 'Shows care for others at times; helps peers when reminded and is learning to consider their feelings.',
    BE: 'Seldom shows awareness of others’ needs; needs guidance to act with kindness and respect.',
  },
  communicator: {
    EE: 'Expresses ideas clearly and confidently in a variety of ways, listens attentively, and responds thoughtfully to others.',
    ME: 'Confidently shares ideas verbally and listens actively when peers are presenting.',
    AE: 'Shares ideas when encouraged; is developing clarity of expression and active listening.',
    BE: 'Finds it difficult to share ideas or listen to others; needs significant support to communicate.',
  },
  reflective: {
    EE: 'Consistently reflects in depth on own learning, identifies strengths and areas to improve, and independently sets meaningful goals.',
    ME: 'Thoughtfully assesses own work during self-reflection sessions and independently identifies areas to improve.',
    AE: 'Reflects on own work with guidance; is beginning to identify areas to improve.',
    BE: 'Rarely reflects on own learning; needs considerable support to recognize strengths and areas to improve.',
  },
  thinking: {
    EE: 'Analyzes complex information critically and creatively to form well-reasoned, original arguments and solutions.',
    ME: 'Effectively analyzes diverse information to form logical, independent arguments.',
    AE: 'Analyzes information with some support; arguments are developing but not yet consistently logical.',
    BE: 'Finds it difficult to analyze information or form arguments; needs step-by-step guidance.',
  },
  social: {
    EE: 'Takes a positive leading role in diverse groups, supports others, and resolves conflicts constructively and independently.',
    ME: 'Cooperates well in diverse group settings and shows positive conflict-resolution skills.',
    AE: 'Cooperates in groups most of the time; is developing the ability to resolve conflicts with support.',
    BE: 'Has difficulty working with others; needs frequent support to cooperate and resolve conflicts.',
  },
  selfmanagement: {
    EE: 'Manages time and tasks independently and efficiently, stays focused, and keeps learning materials well organized.',
    ME: 'Stays on task and maintains organization of learning materials.',
    AE: 'Stays on task with reminders; is developing organization of time and materials.',
    BE: 'Often off task and disorganized; needs constant reminders and support to manage time and materials.',
  },
};

const BY_NAME = {
  inquirer: 'inquirer', caring: 'caring', communicator: 'communicator', communicators: 'communicator', reflective: 'reflective',
  thinkingskills: 'thinking', thinking: 'thinking', socialskills: 'social', social: 'social',
  selfmanagement: 'selfmanagement', selfmanagementskills: 'selfmanagement',
};

/** Contoh untuk satu atribut (lewat id bawaan atau namanya), atau null. */
export function atlExamplesFor(attr) {
  if (ATL_LEVEL_EXAMPLES[attr.id]) return ATL_LEVEL_EXAMPLES[attr.id];
  const key = BY_NAME[String(attr.name || '').toLowerCase().replace(/[^a-z]/g, '')];
  return key ? ATL_LEVEL_EXAMPLES[key] : null;
}
