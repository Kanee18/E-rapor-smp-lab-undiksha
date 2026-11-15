// Layar kunci, kode pemulihan, dan isian kata sandi.
// Penegakan kunci ada di Rust (perintah data ditolak selama terkunci); layar ini hanya pintunya.
import { api } from './api.js';
import { h, errorText, icon, link, openModal, toast } from './dom.js';

/** Isian kata sandi dengan tombol lihat/sembunyikan. */
export function passwordField(label, { autocomplete = 'current-password', placeholder = '' } = {}) {
  const input = h('input', { type: 'password', autocomplete, placeholder, spellcheck: 'false', 'aria-label': label });
  const eye = h('button', { type: 'button', class: 'icon-btn eye', title: 'Tampilkan kata sandi', 'aria-label': 'Tampilkan kata sandi',
    onclick: () => {
      const show = input.type === 'password';
      input.type = show ? 'text' : 'password';
      eye.replaceChildren(icon(show ? 'eyeOff' : 'eye'));
      input.focus();
    } }, icon('eye'));
  const el = h('label', { class: 'field' }, h('span', { class: 'lbl' }, label), h('span', { class: 'pw' }, input, eye));
  return { el, input };
}

/**
 * Tampilkan kode pemulihan sekali. Resolve setelah pengguna menyatakan sudah menyimpannya.
 */
export function showRecoveryCode(code, { title = 'Simpan kode pemulihan' } = {}) {
  return new Promise((resolve) => {
    let done = false;
    const finish = (close) => { if (done) return; done = true; close?.(); resolve(); };
    const ack = h('input', { type: 'checkbox', onchange: () => modal.setActions(actions(ack.checked)) });
    const actions = (ok) => [
      { label: 'Salin kode', onClick: async () => {
        try { await navigator.clipboard.writeText(code); toast('Kode disalin', 'ok'); } catch { toast('Tidak bisa menyalin; catat manual saja', 'error'); }
      } },
      { label: 'Selesai', kind: 'primary', disabled: !ok, onClick: (close) => finish(close) },
    ];
    const modal = openModal({
      title,
      body: h('div', { class: 'stack' },
        h('p', {}, 'Kalau kata sandi lupa, aplikasi hanya bisa dibuka dengan kode ini. Kode ini ', h('b', {}, 'hanya ditampilkan sekali'), '.'),
        h('div', { class: 'recovery-code', 'aria-label': 'Kode pemulihan' }, code),
        h('ul', { class: 'hint', style: { margin: 0, paddingLeft: '18px' } },
          h('li', {}, 'Tulis di kertas dan simpan di tempat aman (mis. lemari kepala sekolah), terpisah dari laptop.'),
          h('li', {}, 'Jangan disimpan di laptop ini atau di flashdisk backup.')),
        h('label', { class: 'check' }, ack, 'Saya sudah mencatat kode ini di tempat aman')),
      actions: actions(false),
      onClose: () => finish(),
    });
  });
}

/**
 * Layar kunci penuh. Resolve setelah berhasil dibuka (kata sandi atau kode pemulihan).
 * @param {{ problem?: string }} status
 */
export function showLockScreen(status = {}) {
  return new Promise((resolve) => {
    document.querySelectorAll('dialog').forEach((d) => d.remove());
    document.querySelector('.lock-screen')?.remove();
    const card = h('div', { class: 'lock-card', role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'lock-title' });
    const overlay = h('div', { class: 'lock-screen' }, card);
    document.body.appendChild(overlay);
    document.body.classList.add('locked');

    const finish = () => {
      document.body.classList.remove('locked');
      overlay.classList.add('out');
      setTimeout(() => overlay.remove(), 220);
      resolve();
    };
    const brand = () => h('div', { class: 'lock-brand' },
      h('img', { src: 'logo-sekolah.png', alt: '', width: 34, height: 48 }),
      h('div', {}, h('div', { class: 'name' }, 'E-Raport'), h('div', { class: 'meta' }, 'SMP Lab Undiksha')));
    const errBox = h('p', { class: 'lock-err', role: 'alert' });
    const fail = (e, input) => {
      errBox.textContent = errorText(e);
      card.classList.remove('shake');
      void card.offsetWidth; // ulangi animasi getar
      card.classList.add('shake');
      input?.select();
    };

    function drawLogin() {
      errBox.textContent = '';
      const pw = passwordField('Kata sandi');
      const submit = h('button', { type: 'submit', class: 'btn primary lock-go' }, 'Buka', icon('arrow'));
      const form = h('form', { class: 'stack', onsubmit: async (ev) => {
        ev.preventDefault();
        if (!pw.input.value) { pw.input.focus(); return; }
        submit.disabled = true;
        try { await api.securityUnlock(pw.input.value); finish(); } catch (e) { fail(e, pw.input); } finally { submit.disabled = false; }
      } }, pw.el, errBox, submit);
      card.replaceChildren(brand(),
        h('div', {}, h('h1', { id: 'lock-title' }, 'Aplikasi terkunci'), h('p', { class: 'muted' }, 'Masukkan kata sandi untuk melanjutkan.')),
        status.problem ? h('p', { class: 'note danger small' }, status.problem) : form,
        status.problem ? null : h('div', { class: 'lock-foot' }, link('Lupa kata sandi?', drawRecovery, 'muted')));
      setTimeout(() => pw.input.focus(), 30);
    }

    function drawRecovery() {
      errBox.textContent = '';
      const code = h('input', { autocomplete: 'off', spellcheck: 'false', placeholder: 'XXXX-XXXX-XXXX-XXXX', class: 'code-input', 'aria-label': 'Kode pemulihan' });
      const pw1 = passwordField('Kata sandi baru', { autocomplete: 'new-password' });
      const pw2 = passwordField('Ulangi kata sandi baru', { autocomplete: 'new-password' });
      const submit = h('button', { type: 'submit', class: 'btn primary lock-go' }, 'Atur kata sandi baru');
      const form = h('form', { class: 'stack', onsubmit: async (ev) => {
        ev.preventDefault();
        if (pw1.input.value !== pw2.input.value) { fail('Kata sandi baru dan ulangannya tidak sama.', pw2.input); return; }
        submit.disabled = true;
        try {
          const fresh = await api.securityResetWithRecovery(code.value, pw1.input.value);
          await showRecoveryCode(fresh, { title: 'Kode pemulihan baru' });
          finish();
        } catch (e) { fail(e, code); } finally { submit.disabled = false; }
      } },
      h('label', { class: 'field' }, h('span', { class: 'lbl' }, 'Kode pemulihan'), code,
        h('span', { class: 'hint' }, 'Kode 16 huruf/angka yang dicatat saat kata sandi dipasang.')),
      pw1.el, pw2.el, errBox, submit);
      card.replaceChildren(brand(),
        h('div', {}, h('h1', { id: 'lock-title' }, 'Lupa kata sandi'), h('p', { class: 'muted' }, 'Pakai kode pemulihan untuk membuat kata sandi baru. Kode lama tidak berlaku lagi setelah ini.')),
        form,
        h('div', { class: 'lock-foot' }, link('Kembali', drawLogin, 'muted')));
      setTimeout(() => code.focus(), 30);
    }

    drawLogin();
  });
}
