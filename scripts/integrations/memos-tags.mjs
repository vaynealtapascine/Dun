// Memos keeps one tag hierarchy separator, flattening any later '/' to '-'.
// Normalize only the supported shapes; validation stays in the reminder parser.
export function reminderParts(tag) {
  let value = tag.toLowerCase();
  const forms = [
    [/^remind\/(\d{4}-\d{2}-\d{2})-(\d{3,4})$/, 'remind/$1/$2'],
    [/^remind\/in-(\d+[hdwmy])$/, 'remind/in/$1'],
    [/^remind\/daily-(\d{3,4})$/, 'remind/daily/$1'],
    [/^remind\/weekly-([a-z]+)(?:-(\d{3,4}))?$/, 'remind/weekly/$1/$2'],
    [/^remind\/monthly-(\d{1,2})(?:-(\d{3,4}))?$/, 'remind/monthly/$1/$2'],
    [/^remind\/yearly-(\d{2}-\d{2})(?:-(\d{3,4}))?$/, 'remind/yearly/$1/$2'],
  ];
  for (const [pattern, replacement] of forms) {
    if (pattern.test(value)) {
      value = value.replace(pattern, replacement).replace(/\/$/, '');
      break;
    }
  }
  return value.split('/').slice(1);
}
