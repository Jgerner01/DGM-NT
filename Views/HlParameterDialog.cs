using System.Windows;
using System.Windows.Controls;
using DGM_NT.Models;
using DGM_NT.Services;

namespace DGM_NT.Views;

/// <summary>
/// Dialog zur Parametrisierung der Höhenlinienerzeugung
/// Ähnlich wie im Delphi-Originalprogramm (Form7 / dlgpar2)
/// </summary>
public class HlParameterDialog : Window
{
    private ComboBox _cmbAequidistanz = null!;
    private TextBox _txtMinHoehe = null!;
    private TextBox _txtMaxHoehe = null!;
    private ComboBox _cmbZaehlkurven = null!;
    
    // Glättung
    private ComboBox _cmbGlaettung = null!;
    private TextBox _txtGlaettungFaktor = null!;
    private CheckBox _chkGlaettungKanten = null!;
    
    private CheckBox _chkBeschriftung = null!;
    private ComboBox _cmbSchriftgroesse = null!;
    
    private TextBlock _vorschauText = null!;
    private double _minH, _maxH;
    
    public double Aequidistanz { get; private set; } = 1.0;
    public double MinHoehe { get; private set; } = 0;
    public double MaxHoehe { get; private set; } = 0;
    public int ZaehlkurvenIntervall { get; private set; } = 5;
    public int GlaettungTyp { get; private set; } = 0; // 0=Keine, 1=Linear, 2=B-Spline, 3=Bezier
    public double GlaettungFaktor { get; private set; } = 1.0;
    public bool GlaettungKanten { get; private set; } = true;
    public bool Beschriftung { get; private set; }
    public double Schriftgroesse { get; private set; } = 2.5;
    
    public HlParameterDialog(double minH, double maxH, double aktuelleAequidistanz = 1.0, int zaehlkurven = 5)
    {
        _minH = minH;
        _maxH = maxH;
        
        // Parameter aus XML-Datei laden (VOR UI-Setup, damit Werte verwendet werden)
        var appParams = AppParameter.Laden();
        
        Title = "Höhenlinien-Parameter";
        Width = 500;
        Height = 620;  // Größer, damit OK-Button sichtbar ist
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;
        Background = new System.Windows.Media.SolidColorBrush(System.Windows.Media.Color.FromRgb(245, 245, 245));
        
        // ScrollViewer für den Fall dass der Bildschirm zu klein ist
        var scrollViewer = new ScrollViewer 
        { 
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled
        };
        
        var mainGrid = new Grid { Margin = new Thickness(20) };
        int row = 0;
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 0: Titel
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 1: Höhenbereich Info
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 2: Äquidistanz
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 3: Min Höhe
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 4: Max Höhe
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 5: Zählkurven
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 6: Separator
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 7: Glättung Typ
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 8: Glättung Faktor
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 9: Glättung Kanten
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 10: Separator
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 11: Beschriftung
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 12: Schriftgröße
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 13: Vorschau
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // 14: Buttons
        
        // Titel
        var titel = new TextBlock
        {
            Text = "⛰️ Höhenlinien-Parameter",
            FontSize = 18,
            FontWeight = FontWeights.Bold,
            Margin = new Thickness(0, 0, 0, 5)
        };
        Grid.SetRow(titel, row++);
        mainGrid.Children.Add(titel);
        
        // Höhenbereich Info
        var infoBorder = new Border
        {
            Background = System.Windows.Media.Brushes.LightBlue,
            BorderBrush = System.Windows.Media.Brushes.SteelBlue,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(10),
            Margin = new Thickness(0, 5, 0, 10)
        };
        var infoPanel = new StackPanel();
        infoPanel.Children.Add(new TextBlock { Text = "Höhenbereich der importierten Punkte:", FontWeight = FontWeights.SemiBold, FontSize = 12 });
        infoPanel.Children.Add(new TextBlock { Text = $"Minimum: {minH:F2} m", FontSize = 12, Margin = new Thickness(0, 2, 0, 0) });
        infoPanel.Children.Add(new TextBlock { Text = $"Maximum: {maxH:F2} m", FontSize = 12 });
        infoPanel.Children.Add(new TextBlock { Text = $"Spanne: {maxH - minH:F2} m", FontSize = 12, FontWeight = FontWeights.Bold });
        infoBorder.Child = infoPanel;
        Grid.SetRow(infoBorder, row++);
        mainGrid.Children.Add(infoBorder);
        
        // Äquidistanz
        var aeqPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        aeqPanel.Children.Add(new Label { Content = "_Äquidistanz (Höhenabstand):", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        _cmbAequidistanz = new ComboBox { FontSize = 13, Padding = new Thickness(5), SelectedIndex = 4 };
        double[] aeqWerte = { 0.1, 0.2, 0.25, 0.5, 1.0, 1.5, 2.0, 2.5, 5.0, 10.0 };
        int selectedIndex = 4;
        for (int i = 0; i < aeqWerte.Length; i++)
        {
            _cmbAequidistanz.Items.Add(new ComboBoxItem { Content = $"{aeqWerte[i]} m", Tag = aeqWerte[i] });
            if (Math.Abs(aeqWerte[i] - aktuelleAequidistanz) < 0.01)
                selectedIndex = i;
        }
        _cmbAequidistanz.SelectedIndex = selectedIndex;
        _cmbAequidistanz.SelectionChanged += (s, e) => VorschauAktualisieren();
        aeqPanel.Children.Add(_cmbAequidistanz);
        Grid.SetRow(aeqPanel, row++);
        mainGrid.Children.Add(aeqPanel);
        
        // Min Höhe
        var minPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        minPanel.Children.Add(new Label { Content = "M_inimale Höhe (leer = automatisch):", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        _txtMinHoehe = new TextBox { FontSize = 12, Padding = new Thickness(5) };
        minPanel.Children.Add(_txtMinHoehe);
        Grid.SetRow(minPanel, row++);
        mainGrid.Children.Add(minPanel);
        
        // Max Höhe
        var maxPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        maxPanel.Children.Add(new Label { Content = "Ma_ximale Höhe (leer = automatisch):", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        _txtMaxHoehe = new TextBox { FontSize = 12, Padding = new Thickness(5) };
        maxPanel.Children.Add(_txtMaxHoehe);
        Grid.SetRow(maxPanel, row++);
        mainGrid.Children.Add(maxPanel);
        
        // Zählkurven-Intervall
        var zkPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        zkPanel.Children.Add(new Label { Content = "_Zählkurven-Intervall (jede n-te Linie dicker):", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        _cmbZaehlkurven = new ComboBox { FontSize = 12, Padding = new Thickness(5), SelectedIndex = 1 };
        int[] zkWerte = { 2, 5, 10 };
        for (int i = 0; i < zkWerte.Length; i++)
        {
            _cmbZaehlkurven.Items.Add(new ComboBoxItem { Content = $"Jede {zkWerte[i]}. Linie", Tag = zkWerte[i] });
            if (zaehlkurven == zkWerte[i])
                _cmbZaehlkurven.SelectedIndex = i;
        }
        _cmbZaehlkurven.SelectionChanged += (s, e) => VorschauAktualisieren();
        zkPanel.Children.Add(_cmbZaehlkurven);
        Grid.SetRow(zkPanel, row++);
        mainGrid.Children.Add(zkPanel);
        
        // Separator
        var sep1 = new Separator { Margin = new Thickness(0, 5, 0, 5) };
        Grid.SetRow(sep1, row++);
        mainGrid.Children.Add(sep1);
        
        // Glättung Typ
        var glPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        glPanel.Children.Add(new Label { Content = "_Glättungsverfahren:", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        _cmbGlaettung = new ComboBox { FontSize = 12, Padding = new Thickness(5) };
        string[] glTypen = { "Keine Glättung (Original)", "Lineare Glättung", "B-Spline Glättung", "Bézier-Glättung" };
        for (int i = 0; i < glTypen.Length; i++)
        {
            _cmbGlaettung.Items.Add(new ComboBoxItem { Content = glTypen[i], Tag = i });
            if (appParams.HlGlaettungTyp == i)
                _cmbGlaettung.SelectedIndex = i;
        }
        if (_cmbGlaettung.SelectedIndex < 0) _cmbGlaettung.SelectedIndex = 0;
        glPanel.Children.Add(_cmbGlaettung);
        Grid.SetRow(glPanel, row++);
        mainGrid.Children.Add(glPanel);
        
        // Glättung Faktor
        var glFaktorPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        glFaktorPanel.Children.Add(new Label { Content = "Glättungs_faktor (0.1 = stark, 1.0 = normal, 5.0 = schwach):", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        _txtGlaettungFaktor = new TextBox { FontSize = 12, Padding = new Thickness(5), Text = appParams.HlGlaettungFaktor.ToString("F1") };
        glFaktorPanel.Children.Add(_txtGlaettungFaktor);
        Grid.SetRow(glFaktorPanel, row++);
        mainGrid.Children.Add(glFaktorPanel);
        
        // Glättung Kanten erhalten
        _chkGlaettungKanten = new CheckBox { Content = "Geländekanten erhalten (Bruchkanten beachten)", FontSize = 12, Margin = new Thickness(0, 0, 0, 5), IsChecked = appParams.HlGlaettungKanten };
        Grid.SetRow(_chkGlaettungKanten, row++);
        mainGrid.Children.Add(_chkGlaettungKanten);
        
        // Separator
        var sep2 = new Separator { Margin = new Thickness(0, 5, 0, 5) };
        Grid.SetRow(sep2, row++);
        mainGrid.Children.Add(sep2);
        
        // Beschriftung
        _chkBeschriftung = new CheckBox { Content = "Höhenlinien beschriften (Höhenwerte anzeigen)", FontSize = 12, Margin = new Thickness(0, 0, 0, 5), IsChecked = appParams.HlBeschriftung };
        Grid.SetRow(_chkBeschriftung, row++);
        mainGrid.Children.Add(_chkBeschriftung);
        
        // Schriftgröße
        var sgPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        sgPanel.Children.Add(new Label { Content = "Schriftgröße für Höhenwerte:", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        _cmbSchriftgroesse = new ComboBox { FontSize = 12, Padding = new Thickness(5), SelectedIndex = 2 };
        double[] sgWerte = { 1.5, 2.0, 2.5, 3.0, 4.0, 5.0 };
        for (int i = 0; i < sgWerte.Length; i++)
        {
            _cmbSchriftgroesse.Items.Add(new ComboBoxItem { Content = $"{sgWerte[i]} m", Tag = sgWerte[i] });
            if (Math.Abs(sgWerte[i] - appParams.HlSchriftgroesse) < 0.01)
                _cmbSchriftgroesse.SelectedIndex = i;
        }
        sgPanel.Children.Add(_cmbSchriftgroesse);
        Grid.SetRow(sgPanel, row++);
        mainGrid.Children.Add(sgPanel);
        
        // Vorschau-Info
        var vorschauBorder = new Border
        {
            Background = System.Windows.Media.Brushes.LightYellow,
            BorderBrush = System.Windows.Media.Brushes.Goldenrod,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(10),
            Margin = new Thickness(0, 5, 0, 10)
        };
        var vorschauPanel = new StackPanel();
        var vorschauLabel = new TextBlock { Text = "📊 Vorschau:", FontWeight = FontWeights.SemiBold, FontSize = 12 };
        _vorschauText = new TextBlock { Text = "", FontSize = 11, TextWrapping = TextWrapping.Wrap };
        vorschauPanel.Children.Add(vorschauLabel);
        vorschauPanel.Children.Add(_vorschauText);
        vorschauBorder.Child = vorschauPanel;
        Grid.SetRow(vorschauBorder, row++);
        mainGrid.Children.Add(vorschauBorder);
        
        VorschauAktualisieren();
        
        // Buttons
        var btnPanel = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 10, 0, 0) };
        
        var btnOk = new Button { Content = "✓ OK – Parameter speichern & Höhenlinien erzeugen", Padding = new Thickness(20, 10, 20, 10), FontSize = 13, FontWeight = FontWeights.SemiBold, IsDefault = true };
        btnOk.Click += BtnOk_Click;
        btnPanel.Children.Add(btnOk);
        
        var btnAbbruch = new Button { Content = "Abbruch", Padding = new Thickness(15, 8, 15, 8), IsCancel = true };
        btnAbbruch.Click += (s, e) => DialogResult = false;
        btnPanel.Children.Add(btnAbbruch);
        
        Grid.SetRow(btnPanel, row++);
        mainGrid.Children.Add(btnPanel);

        // MainGrid in ScrollViewer packen
        scrollViewer.Content = mainGrid;
        Content = scrollViewer;
    }
    
    private void VorschauAktualisieren()
    {
        if (_cmbAequidistanz.SelectedItem is ComboBoxItem item && item.Tag is double aeq)
        {
            double minH = _minH;
            double maxH = _maxH;
            int anzLinien = (int)((maxH - minH) / aeq) + 1;
            int zkInt = _cmbZaehlkurven.SelectedItem is ComboBoxItem zkItem && zkItem.Tag is int zk ? zk : 5;
            int zaehlKurven = anzLinien / zkInt;
            string glTypName = _cmbGlaettung.SelectedItem is ComboBoxItem glItem && glItem.Content is string s ? s : "Keine";
            _vorschauText.Text = $"Äquidistanz: {aeq} m\n" +
                                $"Anzahl Höhenlinien: ~{anzLinien}\n" +
                                $"Davon Zählkurven: ~{zaehlKurven}\n" +
                                $"Glättung: {glTypName}";
        }
    }
    
    private void BtnOk_Click(object sender, RoutedEventArgs e)
    {
        // Werte lesen
        if (_cmbAequidistanz.SelectedItem is ComboBoxItem aeqItem && aeqItem.Tag is double aeq)
            Aequidistanz = aeq;
        
        double minVal = 0, maxVal = 0;
        double.TryParse(_txtMinHoehe.Text, out minVal);
        double.TryParse(_txtMaxHoehe.Text, out maxVal);
        MinHoehe = minVal;
        MaxHoehe = maxVal;
        
        if (_cmbZaehlkurven.SelectedItem is ComboBoxItem zkItem && zkItem.Tag is int zk)
            ZaehlkurvenIntervall = zk;
        
        if (_cmbGlaettung.SelectedItem is ComboBoxItem glItem && glItem.Tag is int glTyp)
            GlaettungTyp = glTyp;
        
        double.TryParse(_txtGlaettungFaktor.Text, out double glFaktor);
        GlaettungFaktor = glFaktor > 0 ? glFaktor : 1.0;
        
        GlaettungKanten = _chkGlaettungKanten.IsChecked == true;
        Beschriftung = _chkBeschriftung.IsChecked == true;
        
        if (_cmbSchriftgroesse.SelectedItem is ComboBoxItem sgItem && sgItem.Tag is double sg)
            Schriftgroesse = sg;
        
        // Parameter in XML-Datei speichern
        var appParams = AppParameter.Laden();
        appParams.HlAequidistanz = Aequidistanz;
        appParams.HlZaehlkurvenIntervall = ZaehlkurvenIntervall;
        appParams.HlGlaettungTyp = GlaettungTyp;
        appParams.HlGlaettungFaktor = GlaettungFaktor;
        appParams.HlGlaettungKanten = GlaettungKanten;
        appParams.HlBeschriftung = Beschriftung;
        appParams.HlSchriftgroesse = Schriftgroesse;
        appParams.Speichern();
        
        System.Diagnostics.Debug.WriteLine($"[HlDialog OK] Gespeichert: GlaettungTyp={GlaettungTyp}, Faktor={GlaettungFaktor}, XML-Pfad: {AppParameter.ParameterPfad}");
        
        DialogResult = true;
        Close();
    }
}
